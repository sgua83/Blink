module bootstrap.coloring;
import bootstrap.names;
import bootstrap.ir_dataflow;
import bootstrap.stage_fusion;
import bootstrap.x64_instruction;
import bootstrap.frame_layout;
import bootstrap.call_layout;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";

const u32 HEAP_GENERATE_EXCEPTIONS=4;
const u32 HEAP_ZERO_MEMORY=8;
const u64 EDGE=1u64;
const u64 AFFINITY=2u64;

struct ColorNode {
    u64 weight;
    u64 degree;
    u64 across_call;
    u64 edge_start;
    u64 affinity_start;
    u64 affinities;
};
struct GraphPair {u64 first;u64 second;u64 kind;};
struct GraphWalk {address rows;u64 count;u64 capacity;};
struct GraphData {
    u64 count;
    u64 registers;
    u64 outgoing;
    address pairs;
    u64 capacity;
    u64 length;
    address adjacency;
    address places;
};

address color_offset(address data,u64 bytes) on cpu {
    return unsafe_address(address_bits(data)+bytes);
}
address node_at(address graph,u64 id) on cpu {
    return color_offset(graph,size_of<GraphData>()+size_of<GraphWalk>()+id*size_of<ColorNode>());
}
address pair_at(address pairs,u64 index) on cpu {
    return color_offset(pairs,index*size_of<GraphPair>());
}
address adjacency_at(address rows,u64 index) on cpu {
    return color_offset(rows,index*size_of<u64>());
}
u64 neighbor_at(address rows,u64 index) on cpu {
    return unsafe_position<u64>(adjacency_at(rows,index));
}

address graph_storage(u64 count,u64 registers) on cpu {
    address places=HeapAlloc(GetProcessHeap(),HEAP_GENERATE_EXCEPTIONS,count*size_of<ColorPlace>());
    address graph=HeapAlloc(GetProcessHeap(),HEAP_GENERATE_EXCEPTIONS|HEAP_ZERO_MEMORY,
        size_of<GraphData>()+size_of<GraphWalk>()+count*size_of<ColorNode>());
    unsafe_position<GraphData>(graph)=GraphData(count,registers,0u64,address(),0u64,0u64,address(),places);
    return graph;
}
void release_graph_memory(address memory) on cpu {
    if(address_bits(memory)!=0u64){HeapFree(GetProcessHeap(),0,memory);}
}
extern "C" void graph_release(address graph) on cpu {
    GraphData data=unsafe_position<GraphData>(graph);
    release_graph_memory(data.pairs);
    release_graph_memory(data.adjacency);
    release_graph_memory(data.places);
    HeapFree(GetProcessHeap(),0,graph);
}

u64 pair_hash(GraphPair pair) on cpu {
    u64 hash=pair.first^(pair.second*0x9e3779b97f4a7c15u64)^(pair.kind*0x94d049bb133111ebu64);
    hash=(hash^(hash>>30u64))*0xbf58476d1ce4e5b9u64;
    hash=(hash^(hash>>27u64))*0x94d049bb133111ebu64;
    return hash^(hash>>31u64);
}
address seek_pair(address pairs,u64 capacity,GraphPair pair) on cpu {
    u64 mask=capacity-1u64;
    u64 index=pair_hash(pair)&mask;
    while(true){
        address slot=pair_at(pairs,index);
        GraphPair previous=unsafe_position<GraphPair>(slot);
        if(previous.kind==0u64 ||
            (previous.first==pair.first && previous.second==pair.second && previous.kind==pair.kind)){
            return slot;
        }
        index=(index+1u64)&mask;
    }
}
address grow_pairs(address old,u64 old_capacity,u64 capacity) on cpu {
    address pairs=HeapAlloc(GetProcessHeap(),HEAP_GENERATE_EXCEPTIONS|HEAP_ZERO_MEMORY,
        capacity*size_of<GraphPair>());
    u64 index=0u64;
    while(index<old_capacity){
        GraphPair pair=unsafe_position<GraphPair>(pair_at(old,index));
        if(pair.kind!=0u64){unsafe_position<GraphPair>(seek_pair(pairs,capacity,pair))=pair;}
        index+=1u64;
    }
    release_graph_memory(old);
    return pairs;
}
void count_neighbor(address graph,u64 node,u64 kind) on cpu {
    address row=node_at(graph,node);
    if(kind==EDGE){unsafe_position<ColorNode>(row).degree+=1u64;}
    else{unsafe_position<ColorNode>(row).affinities+=1u64;}
}
void graph_pair(address graph,u64 first,u64 second,u64 kind) on cpu {
    // Build each unordered pair once. Self affinities still have one member.
    GraphPair pair=first<second ? GraphPair(first,second,kind) : GraphPair(second,first,kind);
    GraphData data=unsafe_position<GraphData>(graph);
    address slot=address();
    if(data.capacity!=0u64){
        slot=seek_pair(data.pairs,data.capacity,pair);
        if(unsafe_position<GraphPair>(slot).kind!=0u64){return;}
    }
    if(data.length>=data.capacity-data.capacity/4u64){
        u64 capacity=data.capacity==0u64 ? 16u64 : data.capacity*2u64;
        data.pairs=grow_pairs(data.pairs,data.capacity,capacity);
        data.capacity=capacity;
        slot=seek_pair(data.pairs,capacity,pair);
    }
    unsafe_position<GraphPair>(slot)=pair;
    data.length+=1u64;
    unsafe_position<GraphData>(graph)=data;
    count_neighbor(graph,first,kind);
    if(first!=second){count_neighbor(graph,second,kind);}
}

u64 reserve_neighbor_rows(address graph,u64 count) on cpu {
    u64 end=0u64;
    u64 node=0u64;
    while(node<count){
        address at=node_at(graph,node);
        ColorNode row=unsafe_position<ColorNode>(at);
        row.edge_start=end;
        row.affinity_start=end+row.degree;
        end=row.affinity_start+row.affinities;
        row.degree=0u64;
        row.affinities=0u64;
        unsafe_position<ColorNode>(at)=row;
        node+=1u64;
    }
    return end;
}
void write_neighbor(address graph,address adjacency,u64 node,u64 neighbor,u64 kind) on cpu {
    address at=node_at(graph,node);
    if(kind==EDGE){
        u64 index=unsafe_position<ColorNode>(at).edge_start+unsafe_position<ColorNode>(at).degree;
        unsafe_position<u64>(adjacency_at(adjacency,index))=neighbor;
        unsafe_position<ColorNode>(at).degree+=1u64;
    }else{
        u64 index=unsafe_position<ColorNode>(at).affinity_start+unsafe_position<ColorNode>(at).affinities;
        unsafe_position<u64>(adjacency_at(adjacency,index))=neighbor;
        unsafe_position<ColorNode>(at).affinities+=1u64;
    }
}
extern "C" bool neighbor_before(address left,address right,address context) on cpu {
    return unsafe_position<u64>(left)<unsafe_position<u64>(right);
}
void graph_seal(address graph) on cpu {
    GraphData data=unsafe_position<GraphData>(graph);
    if(data.length==0u64){return;}
    u64 count=reserve_neighbor_rows(graph,data.count);
    address adjacency=HeapAlloc(GetProcessHeap(),HEAP_GENERATE_EXCEPTIONS,count*size_of<u64>());
    u64 index=0u64;
    while(index<data.capacity){
        GraphPair pair=unsafe_position<GraphPair>(pair_at(data.pairs,index));
        if(pair.kind!=0u64){
            write_neighbor(graph,adjacency,pair.first,pair.second,pair.kind);
            if(pair.first!=pair.second){write_neighbor(graph,adjacency,pair.second,pair.first,pair.kind);}
        }
        index+=1u64;
    }
    // Pair deduplication ends at the last interference producer. Both coloring
    // modes then borrow the same ordered rows.
    release_graph_memory(data.pairs);
    unsafe_position<GraphData>(graph).pairs=address();
    unsafe_position<GraphData>(graph).adjacency=adjacency;
    u64 node=0u64;
    while(node<data.count){
        ColorNode row=unsafe_position<ColorNode>(node_at(graph,node));
        if(row.degree>1u64){
            sort_rows<u64>(adjacency_at(adjacency,row.edge_start),row.degree,address(),c_function(neighbor_before));
        }
        if(row.affinities>1u64){
            sort_rows<u64>(adjacency_at(adjacency,row.affinity_start),row.affinities,address(),c_function(neighbor_before));
        }
        node+=1u64;
    }
}

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
        if (place.kind == ColorPlaceKind::Stack) {
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
        if (unsafe_position<ColorPlace>(row).kind == ColorPlaceKind::Stack) {
            slot = unsafe_position<ColorPlace>(row).value;
            unsafe_position<ColorPlace>(row).value = unsafe_position<u64>(order_at(scratch, slot));
        }
        id += 1u64;
    }
    return end;
}

u64 packet_order(address graph,u64 count,address places,address order,Span uses) on cpu {
    address adjacency=unsafe_position<GraphData>(graph).adjacency;
    u64 first_skipped = count;
    u64 id = 0u64;
    while (id < count) {
        bool index_only = id < uses.count && unsafe_position<u8>(
            unsafe_address(address_bits(uses.data) + id * u64(uses.stride))) == u8(1);
        unsafe_position<ColorPlace>(place_at(places, id)) =
            ColorPlace(0u64, ColorPlaceKind::Unseen, index_only ? u8(0) : u8(16), u8(0));
        if (index_only) {
            first_skipped = id < first_skipped ? id : first_skipped;
        } else {
            ColorNode facts = unsafe_position<ColorNode>(node_at(graph,id));
            unsafe_position<array<u64,3>>(order_at(order, id)) = array<u64,3>(facts.weight, facts.degree, id);
        }
        id += 1u64;
    }

    // The original graph is symmetric. Discount each edge from a skipped
    // register to an active neighbor in that neighbor's actual priority row.
    id = first_skipped;
    while (id < uses.count) {
        if (unsafe_position<ColorPlace>(place_at(places, id)).width == u8(0)) {
            ColorNode skipped=unsafe_position<ColorNode>(node_at(graph,id));
            u64 at=0u64;
            while (at<skipped.degree) {
                u64 neighbor=neighbor_at(adjacency,skipped.edge_start+at);
                if (unsafe_position<ColorPlace>(place_at(places, neighbor)).width != u8(0)) {
                    address row = order_at(order, neighbor);
                    array<u64,3> priority = unsafe_position<array<u64,3>>(row);
                    priority[1] -= 1u64;
                    unsafe_position<array<u64,3>>(row) = priority;
                }
                at+=1u64;
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

ColorLayout color_locations(address graph,address places,u32 mode,Span packet_uses) on cpu {
    GraphData data=unsafe_position<GraphData>(graph);
    u64 count=data.count;
    address order=address();
    if(count!=0u64){
        order=HeapAlloc(GetProcessHeap(),HEAP_GENERATE_EXCEPTIONS,count*size_of<array<u64,3>>());
    }
    u64 pending = 0u64;
    u64 id = 0u64;
    if (mode == 2) {
        pending = packet_order(graph, count, places, order, packet_uses);
    } else {
        while (id < count) {
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, id));
            // Addressable fields receive their stack positions after coloring.
            if (place.kind != ColorPlaceKind::Immediate && place.width != 0) {
                ColorNode facts = unsafe_position<ColorNode>(node_at(graph,id));
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
        ColorNode facts = unsafe_position<ColorNode>(node_at(graph,id));
        address destination = place_at(places, id);
        ColorPlaceKind bank = ColorPlaceKind::Gpr;
        u32 allowed = mode == 1 ? 0x3e0 : 0xfe0;
        if (facts.across_call != 0u64) { allowed &= 0xe0; }
        if (unsafe_position<ColorPlace>(destination).floating != 0) {
            bank = ColorPlaceKind::Xmm;
            allowed = facts.across_call != 0u64 ? 0xffc0 : 0xfffc;
        }
        if (mode == 2) { bank = ColorPlaceKind::Xmm; allowed = 0xfff0; }

        u32 used = 0;
        u64 visit = at + 1u64;
        u64 edge=0u64;
        while (edge<facts.degree) {
            u64 neighbor=neighbor_at(data.adjacency,facts.edge_start+edge);
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, neighbor));
            if (place.kind == bank) { used |= 1 << u32(place.value); }
            if (place.kind == ColorPlaceKind::Stack) {
                unsafe_position<u64>(spill_mark(order, u32(place.value))) = visit;
            }
            edge+=1u64;
        }
        u32 available = allowed & ~used;
        u32 chosen = 16;
        u64 affinity=0u64;
        while (affinity<facts.affinities) {
            u64 neighbor=neighbor_at(data.adjacency,facts.affinity_start+affinity);
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, neighbor));
            if (place.kind == bank && (available & (1 << u32(place.value))) != 0) {
                chosen = u32(place.value);
                break;
            }
            affinity+=1u64;
        }
        if (chosen == 16) {
            u32 candidate = 0;
            while (candidate < 16) {
                u32 reg = candidate;
                if (bank == ColorPlaceKind::Gpr) {
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
            if (bank == ColorPlaceKind::Gpr) { saved_gpr |= (1 << chosen) & 0xe0; }
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
            unsafe_position<ColorPlace>(destination).kind = ColorPlaceKind::Stack;
        }
        at += 1u64;
    }
    u64 base=mode==2 ? 0u64 : data.outgoing;
    u64 end=base;
    if(spills!=0){end=layout_spills(places,count,order,u64(spills),base);}
    release_graph_memory(order);
    return ColorLayout(end,saved_gpr,saved_xmm);
}

extern "C" ScalarAllocation finish_scalar(address graph,u32 mode,address locals,
    cfn<LocalLayout,address,usize> ensures memory(read) local_layout) on cpu requires nonzero(local_layout) {
    graph_seal(graph);
    GraphData data=unsafe_position<GraphData>(graph);
    ColorLayout layout=color_locations(graph,data.places,mode,Span(address(),0u64,0u64));
    layout.end=place_private(data,layout.end,locals,local_layout);
    unsafe_position<GraphData>(graph).places=address();
    ColorPositions positions=ColorPositions(Span(data.places,data.count,size_of<ColorPlace>()),data.registers);
    return ScalarAllocation(positions,layout);
}

struct PreparedPacket { Span code; Span places; ColorLayout layout; };

extern "C" bool prepare_packet(address graph, address context, Span code, PacketPlanHeader header, address output, PacketPlanHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.stride) requires nonzero(host.written) requires nonzero(host.scalar)
    requires nonzero(host.widths) {
    Span empty = Span(address(), 0u64, 0u64);
    PacketPlanProduct plan = PacketPlanProduct(empty, empty);
    if (!packet_plan(context, code, header, address_of(plan), host)) { return false; }

    u64 count = unsafe_position<GraphData>(graph).count;
    address places = HeapAlloc(GetProcessHeap(), HEAP_GENERATE_EXCEPTIONS, count * size_of<ColorPlace>());
    ColorLayout layout = color_locations(graph, places, 2, plan.uses);
    release_graph_memory(plan.uses.data);
    unsafe_position<PreparedPacket>(output) = PreparedPacket(plan.code, Span(places, count, size_of<ColorPlace>()), layout);
    return true;
}

extern "C" void color_places_drop(Span places) on cpu {
    HeapFree(GetProcessHeap(), 0, places.data);
}

u64 place_private(GraphData data, u64 data_end, address locals,
    cfn<LocalLayout,address,usize> ensures memory(read) local_layout) on cpu requires nonzero(local_layout) {
    u64 end = data_end;
    usize slot = data.registers;
    while (slot < data.count) {
        ColorPlace first = unsafe_position<ColorPlace>(place_at(data.places, slot));
        if (first.width != u8(0) || first.value == 0u64) {
            slot += 1u64;
            continue;
        }
        // The addressable declaration's first row still holds its local end.
        usize limit = first.value;
        LocalLayout layout = local_layout(locals, limit);
        u64 alignment = max(16u64, u64(layout.alignment));
        u64 base = ((end + alignment - 1u64) / alignment) * alignment;
        Span offsets = layout.offsets;
        usize index = 0u64;
        while (index < offsets.count) {
            address offset = color_offset(offsets.data, index * u64(offsets.stride));
            address place = place_at(data.places, slot + index);
            unsafe_position<ColorPlace>(place).value = base + u64(unsafe_position<u32>(offset));
            unsafe_position<ColorPlace>(place).kind = ColorPlaceKind::Stack;
            index += 1u64;
        }
        end = base + u64(layout.size);
        slot = data.registers + limit;
    }
    return end;
}

void record_definition(address place, u32 literal, u64 bits) on cpu {
    ColorPlaceKind kind = unsafe_position<ColorPlace>(place).kind;
    if (literal != 0 && (kind == ColorPlaceKind::Unseen
        || (kind == ColorPlaceKind::Immediate && unsafe_position<ColorPlace>(place).value == bits))) {
        unsafe_position<ColorPlace>(place).kind = ColorPlaceKind::Immediate;
        unsafe_position<ColorPlace>(place).value = bits;
    } else {
        unsafe_position<ColorPlace>(place).kind = ColorPlaceKind::Varying;
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
u64 graph_slot_id(address graph,GraphSpace space,u64 index) on cpu {
    return space==GraphSpace::Register ? index : unsafe_position<GraphData>(graph).registers+index;
}
void graph_edge(address graph,u64 first,u64 second) on cpu {
    if(first!=second){graph_pair(graph,first,second,EDGE);}
}
void graph_weight(address graph,u64 id,u64 amount) on cpu {
    address row=node_at(graph,id);
    unsafe_position<ColorNode>(row).weight=graph_saturating_add(unsafe_position<ColorNode>(row).weight,amount);
}
extern "C" void graph_live_event(address graph,address live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu
    requires nonzero(core.visit_set) {
    u64 id=graph_slot_id(graph,space,index);
    if(event.kind==0){
        if((event.skip_present==0 || id!=event.skip) && unsafe_position<ColorPlace>(place_at(unsafe_position<GraphData>(graph).places,id)).width!=0){
            graph_edge(graph,event.anchor,id);
        }
    } else if(event.kind==1){
        if(unsafe_position<ColorPlace>(place_at(unsafe_position<GraphData>(graph).places,id)).width!=0){
            core.visit_set(live,ordinal,graph,GraphEvent(id,0u64,0,0));
        }
    } else {unsafe_position<ColorNode>(node_at(graph,id)).across_call=1u64;}
}
extern "C" void graph_together(address graph,address live,GraphCore core) on cpu
    requires nonzero(core.visit_set) {
    core.visit_set(live,18446744073709551615u64,graph,GraphEvent(0u64,0u64,1,0));
}
extern "C" void graph_inputs(address graph,address inputs,u64 count) on cpu {
    u64 index=0u64;
    while(index<count){
        u64 first=neighbor_at(inputs,index);
        u64 before=0u64;
        while(before<index){
            graph_edge(graph,first,neighbor_at(inputs,before));
            before+=1u64;
        }
        index+=1u64;
    }
}
struct GraphDefinition { u64 weight; u64 transfer_dst; u64 transfer_src; u64 bits; u32 transfer; u32 literal; u32 pass; };
void graph_definition(address graph,address live,GraphSpace space,u64 index,GraphDefinition definition,GraphCore core,LiveHost slots) on cpu
    requires nonzero(core.visit_set) requires nonzero(slots.change) {
    if(definition.pass!=0){slots.change(LiveSetAction::RemoveRegister,live,address(),index);return;}
    if(space==GraphSpace::Register){record_definition(place_at(unsafe_position<GraphData>(graph).places,index),definition.literal,definition.bits);}
    u64 id=graph_slot_id(graph,space,index);
    if(unsafe_position<ColorPlace>(place_at(unsafe_position<GraphData>(graph).places,id)).width!=0){
        GraphEvent event=GraphEvent(id,definition.transfer_src,0,0);
        if(definition.transfer!=0 && id==definition.transfer_dst){event.skip_present=1;}
        core.visit_set(live,18446744073709551615u64,graph,event);
    }
    graph_weight(graph,id,definition.weight);
}
extern "C" void graph_register_read(address graph,usize index,u64 weight) on cpu {
    graph_weight(graph,index,weight);
}
struct GraphOperand {address graph;address live;u64 weight;};
Node graph_node_at(Span span, usize index, u32 kind) on cpu {
    return Node(unsafe_address(address_bits(span.data)+u64(index)*u64(span.stride)),kind);
}
bool graph_indexed_reference(Node reference, Host ir) on cpu requires nonzero(ir.inspect) {
    return ir.inspect(ir.inspect(reference).left).tag==STORAGE_INDEXED;
}
bool graph_call_uses_r15(Span arguments, Host ir) on cpu requires nonzero(ir.inspect) {
    usize index=0u64;
    while(index<arguments.count){
        View argument=ir.inspect(graph_node_at(arguments,index,N_ARGUMENT));
        if(argument.tag==ARGUMENT_REFERENCE && graph_indexed_reference(argument.left,ir)){return true;}
        index+=1u64;
    }
    return false;
}
u32 graph_instruction(address graph,View node,Span registers,address live,
    u64 weight,GraphHost host) on cpu
    requires nonzero(host.core.visit_set)
    requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.register_operand) requires nonzero(host.live.change) {
    u32 uses_r15=0;
    u64 outgoing=0u64;
    if(node.tag==OP_INVOKE){
        View call=host.live.ir.inspect(node.left);
        outgoing=outgoing_bytes(call,host.live.ir);
        if(graph_call_uses_r15(call.first,host.live.ir)){uses_r15=1;}
    } else if(node.tag==OP_POSITION_ADDRESS){
        if(graph_indexed_reference(node.left,host.live.ir)){uses_r15=1;}
    } else if(node.tag==OP_TEXTURE_BINDING_ADDRESS || (node.tag>=OP_TEXTURE_SIZE && node.tag<=OP_TEXTURE_FETCH) ||
        node.tag==OP_VIEW_LOAD || node.tag==OP_LOAD_INDEXED || node.tag==OP_STORE_INDEXED || node.tag==OP_ATOMIC){
        uses_r15=1;
    } else if(node.tag==OP_PRINT){outgoing=32u64;}
    else if(node.tag==OP_MATH){outgoing=scalar_math_outgoing(ScalarMathOperation(u32(node.b)));}

    u32 transfer=0;
    u64 destination=node.a;
    u64 source=node.b;
    if(node.tag==OP_LOAD_LOCAL){transfer=1;source+=unsafe_position<GraphData>(graph).registers;}
    else if(node.tag==OP_STORE_LOCAL){transfer=1;destination+=unsafe_position<GraphData>(graph).registers;}
    else if(node.tag==OP_CAST && bit_copy(graph_scalar(registers,node.b,host.scalar),graph_scalar(registers,node.a,host.scalar))){
        transfer=1;
    }
    if(transfer!=0 && (unsafe_position<ColorPlace>(place_at(unsafe_position<GraphData>(graph).places,destination)).width==0 ||
        unsafe_position<ColorPlace>(place_at(unsafe_position<GraphData>(graph).places,source)).width==0)){transfer=0;}
    u32 preferred=transfer;
    u64 preferred_destination=destination;
    u64 preferred_source=source;
    if(preferred==0){
        if(node.tag==OP_UNARY){preferred=1;}
        else if(node.tag==OP_SELECT_VALUE){preferred=1;preferred_source=node.d;}
        else if(node.tag==OP_BINARY){
            preferred=1;
            if(node.left.kind==N_COMPARISON){
                u32 comparison=host.live.ir.inspect(node.left).tag;
                address places=unsafe_position<GraphData>(graph).places;
                bool floating=unsafe_position<ColorPlace>(place_at(places,node.b)).floating!=0;
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
        graph_pair(graph,preferred_destination,preferred_source,AFFINITY);
    }
    u32 literal=node.tag==OP_CONSTANT ? 1 : 0;
    GraphDefinition definition=GraphDefinition(weight,destination,source,u64(node.b),transfer,literal,0);
    if(node.tag==OP_STORE_LOCAL){
        graph_definition(graph,live,GraphSpace::Local,node.a,definition,host.core,host.live);
        host.live.change(LiveSetAction::RemoveLocal,live,address(),node.a);
    }else{
        Results results=view_results(node,host.live.ir);
        for(pass in 0..2){
            definition.pass=u32(pass);
            Results cursor=results;
            Word next=next_result(cursor);
            while(next.present!=0){
                graph_definition(graph,live,GraphSpace::Register,next.value,definition,host.core,host.live);
                next=next_result(cursor);
            }
        }
    }
    if(node.tag==OP_INVOKE || outgoing!=0u64){
        if(outgoing>unsafe_position<GraphData>(graph).outgoing){
            unsafe_position<GraphData>(graph).outgoing=outgoing;
        }
        host.core.visit_set(live,18446744073709551615u64,graph,GraphEvent(0u64,0u64,2,0));
    }
    GraphOperand operand=GraphOperand(graph,live,weight);
    visit_instruction(node,host.live.ir,address_of(operand),host.register_operand);
    Locals local_reads=view_locals(node,0,host.live.ir);
    Word next=next_local(local_reads,host.live.ir);
    while(next.present!=0){
        graph_weight(graph,graph_slot_id(graph,GraphSpace::Local,next.value),weight);
        host.live.change(LiveSetAction::InsertLocal,live,address(),next.value);
        next=next_local(local_reads,host.live.ir);
    }
    return uses_r15;
}
struct WalkBlock {Span code;address live;address breaks;address continues;address exits;u64 weight;};
struct WalkJoin {address live;address yes;u64 test;u64 weight;};
struct WalkCondition {WalkBlock block;address before;address header;u64 test;};
struct WalkReplace {address live;address before;address header;};
enum GraphWork:u32 {
    Block(WalkBlock)=0;
    Join(WalkJoin)=1;
    Condition(WalkCondition)=2;
    Replace(WalkReplace)=3;
};

void push_walk(address work,GraphWork step) on cpu {
    GraphWalk list=unsafe_position<GraphWalk>(work);
    if(list.count==list.capacity){
        handle heap=GetProcessHeap();
        u64 capacity=list.capacity==0u64 ? 8u64 : list.capacity*2u64;
        u64 bytes=capacity*size_of<GraphWork>();
        if(list.capacity==0u64){list.rows=HeapAlloc(heap,HEAP_GENERATE_EXCEPTIONS,bytes);}
        else{list.rows=HeapReAlloc(heap,HEAP_GENERATE_EXCEPTIONS,list.rows,bytes);}
        list.capacity=capacity;
    }
    unsafe_position<GraphWork>(color_offset(list.rows,list.count*size_of<GraphWork>()))=step;
    list.count+=1u64;
    unsafe_position<GraphWalk>(work)=list;
}
GraphWork pop_walk(address work) on cpu {
    unsafe_position<GraphWalk>(work).count-=1u64;
    GraphWalk list=unsafe_position<GraphWalk>(work);
    return unsafe_position<GraphWork>(color_offset(list.rows,list.count*size_of<GraphWork>()));
}
void save_walk_tail(address work,WalkBlock block,u64 remaining) on cpu {
    if(remaining!=0u64){
        Span tail=Span(block.code.data,remaining,block.code.stride);
        push_walk(work,GraphWork::Block(WalkBlock(tail,block.live,block.breaks,block.continues,block.exits,block.weight)));
    }
}
void walk_branch(address work,View node,WalkBlock block,GraphHost host) on cpu requires nonzero(host.live.clone_set) {
    address yes=host.live.clone_set(block.live);
    push_walk(work,GraphWork::Join(WalkJoin(block.live,yes,node.a,block.weight)));
    push_walk(work,GraphWork::Block(WalkBlock(node.second,block.live,block.breaks,block.continues,block.exits,block.weight)));
    push_walk(work,GraphWork::Block(WalkBlock(node.first,yes,block.breaks,block.continues,block.exits,block.weight)));
}
void walk_loop(address work,View node,WalkBlock block,address empty,GraphHost host) on cpu
    requires nonzero(host.live.ir.inspect) requires nonzero(host.live.insert_register)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change) {
    address header=private_loop(node.first,node.a,node.second,block.live,block.breaks,block.continues,block.exits,empty,host.live);
    address before=host.live.clone_set(header);
    u64 weight=graph_nested_weight(block.weight);
    WalkBlock condition=WalkBlock(node.first,block.live,block.breaks,block.continues,block.exits,weight);
    push_walk(work,GraphWork::Condition(WalkCondition(condition,before,header,node.a)));
    push_walk(work,GraphWork::Block(WalkBlock(node.second,before,block.live,header,block.exits,weight)));
}
void walk_scope(address work,View node,WalkBlock block,GraphHost host) on cpu requires nonzero(host.live.clone_set) {
    address before=host.live.clone_set(block.live);
    push_walk(work,GraphWork::Replace(WalkReplace(block.live,before,address())));
    push_walk(work,GraphWork::Block(WalkBlock(node.first,before,block.breaks,block.continues,block.live,block.weight)));
}
void walk_join(address graph,WalkJoin join,GraphHost host) on cpu
    requires nonzero(host.live.change) requires nonzero(host.live.drop_set) requires nonzero(host.live.insert_register) {
    host.live.change(LiveSetAction::DrainUnion,join.live,join.yes,0u64);
    host.live.drop_set(join.yes);
    host.live.insert_register(join.live,join.test);
    graph_weight(graph,join.test,join.weight);
}
void walk_condition(address graph,address work,WalkCondition condition,GraphHost host) on cpu
    requires nonzero(host.live.change) requires nonzero(host.live.insert_register) {
    WalkBlock block=condition.block;
    host.live.change(LiveSetAction::DrainUnion,condition.before,block.live,0u64);
    host.live.insert_register(condition.before,condition.test);
    graph_weight(graph,condition.test,block.weight);
    push_walk(work,GraphWork::Replace(WalkReplace(block.live,condition.before,condition.header)));
    push_walk(work,GraphWork::Block(WalkBlock(block.code,condition.before,block.breaks,block.continues,block.exits,block.weight)));
}
void walk_replace(WalkReplace replace,GraphHost host) on cpu
    requires nonzero(host.live.change) requires nonzero(host.live.drop_set) {
    host.live.change(LiveSetAction::Swap,replace.live,replace.before,0u64);
    host.live.drop_set(replace.before);
    if(replace.header!=address()){host.live.drop_set(replace.header);}
}
u32 walk_block(address graph,address work,WalkBlock block,Span registers,address empty,GraphHost host) on cpu
    requires nonzero(host.core.visit_set)
    requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change)
    requires nonzero(host.live.insert_register) requires nonzero(host.register_operand) {
    u32 uses_r15=0;
    u64 remaining=block.code.count;
    while(remaining!=0u64){
        remaining-=1u64;
        Node instruction=graph_node_at(block.code,remaining,N_INSTRUCTION);
        View node=host.live.ir.inspect(instruction);
        if(node.tag==OP_BRANCH){
            save_walk_tail(work,block,remaining);
            walk_branch(work,node,block,host);
            return uses_r15;
        }
        if(node.tag==OP_LOOP_VALUE){
            save_walk_tail(work,block,remaining);
            walk_loop(work,node,block,empty,host);
            return uses_r15;
        }
        if(node.tag==OP_SCOPE){
            save_walk_tail(work,block,remaining);
            walk_scope(work,node,block,host);
            return uses_r15;
        }
        if(node.tag==OP_RAISE){
            host.live.change(LiveSetAction::Clear,block.live,address(),0u64);
            host.live.insert_register(block.live,node.a);
            graph_weight(graph,node.a,block.weight);
        }else if(node.tag==OP_EXIT_VALUE){host.live.change(LiveSetAction::Copy,block.live,block.exits,0u64);}
        else if(node.tag==OP_BREAK_VALUE){host.live.change(LiveSetAction::Copy,block.live,block.breaks,0u64);}
        else if(node.tag==OP_CONTINUE_VALUE){host.live.change(LiveSetAction::Copy,block.live,block.continues,0u64);}
        else{uses_r15|=graph_instruction(graph,node,registers,block.live,block.weight,host);}
    }
    return uses_r15;
}
u32 walk_step(address graph,address work,GraphWork step,Span registers,address empty,GraphHost host) on cpu
    requires nonzero(host.core.visit_set)
    requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change)
    requires nonzero(host.live.insert_register) requires nonzero(host.register_operand) {
    match(step){
        GraphWork::Block(block){return walk_block(graph,work,block,registers,empty,host);}
        GraphWork::Join(join){walk_join(graph,join,host);}
        GraphWork::Condition(condition){walk_condition(graph,work,condition,host);}
        GraphWork::Replace(replace){walk_replace(replace,host);}
    }
    return 0;
}
u32 walk_graph(address graph,GraphRequest input,GraphHost host) on cpu
    requires nonzero(host.core.visit_set)
    requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change)
    requires nonzero(host.live.insert_register) requires nonzero(host.register_operand) {
    address work=color_offset(graph,size_of<GraphData>());
    WalkBlock root=WalkBlock(input.code,input.live,input.empty,input.empty,input.empty,1u64);
    u32 uses_r15=walk_block(graph,work,root,input.registers,input.empty,host);
    while(unsafe_position<GraphWalk>(work).count!=0u64){
        GraphWork step=pop_walk(work);
        uses_r15|=walk_step(graph,work,step,input.registers,input.empty,host);
    }
    release_graph_memory(unsafe_position<GraphWalk>(work).rows);
    unsafe_position<GraphWalk>(work)=GraphWalk(address(),0u64,0u64);
    return uses_r15;
}
ScalarEncoding graph_scalar(Span scalars,usize index,cfn<ScalarEncoding,address> ensures memory(read) scalar) on cpu
    requires nonzero(scalar) {
    return scalar(color_offset(scalars.data,index*u64(scalars.stride)));
}

usize initialize_scalar_span(address places,usize first,Span scalars,
    cfn<ScalarEncoding,address> ensures memory(read) scalar) on cpu requires nonzero(scalar) {
    usize index=0u64;
    while(index<scalars.count){
        ScalarEncoding input=graph_scalar(scalars,index,scalar);
        unsafe_position<ColorPlace>(place_at(places,first+index))=
            ColorPlace(0u64,ColorPlaceKind::Unseen,u8(input.width/8),input.kind==ScalarKind::Float?u8(1):u8(0));
        index+=1u64;
    }
    return first+index;
}

extern "C" void mark_private(address graph, usize begin, usize end) on cpu {
    GraphData data = unsafe_position<GraphData>(graph);
    address first = place_at(data.places, data.registers + begin);
    if (unsafe_position<ColorPlace>(first).value == end) { return; }
    unsafe_position<ColorPlace>(first).value = end;
    usize slot = begin;
    while (slot < end) {
        unsafe_position<ColorPlace>(place_at(data.places, data.registers + slot)).width = u8(0);
        slot += 1u64;
    }
}

void initialize_scalar_places(address graph,GraphRequest input,GraphHost host) on cpu
    requires nonzero(host.scalar) requires nonzero(host.next_local) requires nonzero(host.live.ir.inspect) {
    address places=unsafe_position<GraphData>(graph).places;
    usize first=initialize_scalar_span(places,0u64,input.registers,host.scalar);
    while(true){
        Span scalars=host.next_local(input.locals);
        if(address_bits(scalars.data)==0u64){break;}
        first=initialize_scalar_span(places,first,scalars,host.scalar);
    }
    private_spans(input.code,host.live.ir,graph,c_function(mark_private));
}

extern "C" GraphBuilt graph_create(address request,GraphHost host) on cpu
    requires nonzero(host.core.visit_set)
    requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.next_local)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change)
    requires nonzero(host.live.insert_register) requires nonzero(host.register_operand) {
    GraphRequest input=unsafe_position<GraphRequest>(request);
    address graph=graph_storage(input.registers.count+input.local_slots,input.registers.count);
    initialize_scalar_places(graph,input,host);
    graph_together(graph,input.live,host.core);
    u32 uses_r15=walk_graph(graph,input,host);
    return GraphBuilt(graph,uses_r15);
}
