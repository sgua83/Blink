module bootstrap.build_graph;
import bootstrap.names;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";

const u64 OUT_OF_RANGE = 1u64;
const u64 DEPENDENCY_CYCLE = 2u64;
const u64 ENTRY_PROVIDER = 3u64;
const u64 ERROR_NOT_ENOUGH_MEMORY = 8u64;

struct GraphUnit { GraphRange dependencies; u64 object; };
struct GraphInput { address units; address edges; u64 count; };
struct Visit { u64 unit; u64 next; };
struct Scratch { address states; address order; address visits; address seen; };
struct Providers { address ids; u64 count; u64 capacity; };

address offset(address data,u64 bytes) on cpu {
    return unsafe_address(address_bits(data)+bytes);
}
u64 word(address data,u64 index) on cpu {
    return unsafe_position<u64>(offset(data,index*8u64));
}
void put_word(address data,u64 index,u64 value) on cpu {
    unsafe_position<u64>(offset(data,index*8u64))=value;
}
GraphUnit unit_at(GraphInput input,u64 index) on cpu {
    return unsafe_position<GraphUnit>(offset(input.units,index*size_of<GraphUnit>()));
}
GraphEdge edge_at(GraphInput input,u64 index) on cpu {
    return unsafe_position<GraphEdge>(offset(input.edges,index*size_of<GraphEdge>()));
}
void put_edge(GraphInput input,u64 index,GraphEdge edge) on cpu {
    unsafe_position<GraphEdge>(offset(input.edges,index*size_of<GraphEdge>()))=edge;
}
GraphRange range_at(address rows,u64 index) on cpu {
    return unsafe_position<GraphRange>(offset(rows,index*size_of<GraphRange>()));
}
void put_range(address rows,u64 index,GraphRange range) on cpu {
    unsafe_position<GraphRange>(offset(rows,index*size_of<GraphRange>()))=range;
}
address capture_slot(address slots,u64 unit) on cpu {
    return offset(slots,unit*size_of<CaptureSlot>());
}
EntryKey entry_key(address entries,u64 index) on cpu {
    return unsafe_position<EntryKey>(offset(entries,index*size_of<EntryKey>()));
}
i32 compare_entries(EntryKey first,EntryKey second,address ids) on cpu {
    i32 path=compare_name(first.path,second.path);
    if (path!=i32(0)) { return path; }
    i32 root=compare_name(first.root,second.root);
    if (root!=i32(0)) { return root; }
    if (first.providers.count!=second.providers.count) {
        return first.providers.count<second.providers.count ? i32(-1) : i32(1);
    }
    u64 index=0u64;
    while (index<first.providers.count) {
        u64 left=word(ids,first.providers.start+index);
        u64 right=word(ids,second.providers.start+index);
        if (left!=right) { return left<right ? i32(-1) : i32(1); }
        index+=1u64;
    }
    return i32(0);
}
struct EntryOrder { address entries; address ids; };
extern "C" bool entry_before(address left,address right,address context) on cpu {
    EntryOrder input=unsafe_position<EntryOrder>(context);
    EntryKey first=entry_key(input.entries,word(left,0u64));
    EntryKey second=entry_key(input.entries,word(right,0u64));
    i32 order=compare_entries(first,second,input.ids);
    return order==i32(0) ? first.unit<second.unit : order<i32(0);
}
extern "C" u64 capture_slots(address entries,address order,u64 entry_count,address ids,address slots,u64 unit_count) on cpu {
    EntryOrder input=EntryOrder(entries,ids);
    sort_rows<u64>(order,entry_count,address_of(input),c_function(entry_before));
    u64 first=0u64;
    while (first<entry_count) {
        EntryKey owner=entry_key(entries,word(order,first));
        first+=1u64;
        while (first<entry_count) {
            EntryKey member=entry_key(entries,word(order,first));
            if (compare_entries(owner,member,ids)!=i32(0)) { break; }
            unsafe_position<CaptureSlot>(capture_slot(slots,owner.unit)).kind=CaptureKind::Shared;
            unsafe_position<CaptureSlot>(capture_slot(slots,member.unit))=CaptureSlot(owner.unit,CaptureKind::Borrowed);
            first+=1u64;
        }
    }
    // The earliest original consumer owns the snapshot. Number captures in
    // original unit order to preserve the actual source read order.
    u64 count=0u64;
    u64 unit=0u64;
    while (unit<unit_count) {
        address slot=capture_slot(slots,unit);
        if (unsafe_position<CaptureSlot>(slot).kind==CaptureKind::Borrowed) {
            u64 owner=unsafe_position<CaptureSlot>(slot).index;
            unsafe_position<CaptureSlot>(slot).index=unsafe_position<CaptureSlot>(capture_slot(slots,owner)).index;
        } else {
            unsafe_position<CaptureSlot>(slot).index=count;
            count+=1u64;
        }
        unit+=1u64;
    }
    return count;
}
Visit visit_at(address visits,u64 index) on cpu {
    return unsafe_position<Visit>(offset(visits,index*size_of<Visit>()));
}
void put_visit(address visits,u64 index,Visit visit) on cpu {
    unsafe_position<Visit>(offset(visits,index*size_of<Visit>()))=visit;
}

GraphPlan failure(u64 status,u64 unit) on cpu {
    return GraphPlan(address(),0u64,status,unit);
}

u64 read_dependencies(GraphSource source,GraphInput input,address external_order) on cpu
    requires nonzero(source.row) requires nonzero(source.edge) {
    u64 unit=0u64;
    u64 at=0u64;
    u64 external_count=0u64;
    while (unit<source.count) {
        GraphRow row=source.row(source.units,unit);
        unsafe_position<GraphUnit>(offset(input.units,unit*size_of<GraphUnit>()))=
            GraphUnit(GraphRange(at,row.count),row.object);
        u64 index=0u64;
        while (index<row.count) {
            GraphEdge edge=source.edge(row.dependencies,index);
            put_edge(input,at,edge);
            if (edge.external!=0u64) {
                put_word(external_order,external_count,at);
                external_count+=1u64;
            }
            at+=1u64;
            index+=1u64;
        }
        unit+=1u64;
    }
    return external_count;
}

GraphPlan dependency_order(GraphInput input,Scratch work) on cpu {
    u64 ordered=0u64;
    u64 root=0u64;
    while (root<input.count) {
        if (word(work.states,root)==0u64) {
            put_word(work.states,root,1u64);
            put_visit(work.visits,0u64,Visit(root,0u64));
            u64 depth=1u64;
            while (depth>0u64) {
                u64 top=depth-1u64;
                Visit visit=visit_at(work.visits,top);
                GraphRange direct=unit_at(input,visit.unit).dependencies;
                if (visit.next==direct.count) {
                    put_word(work.states,visit.unit,2u64);
                    put_word(work.order,ordered,visit.unit);
                    ordered+=1u64;
                    depth-=1u64;
                } else {
                    GraphEdge edge=edge_at(input,direct.start+visit.next);
                    put_visit(work.visits,top,Visit(visit.unit,visit.next+1u64));
                    if (edge.external==0u64) {
                        if (edge.index>=input.count) { return failure(OUT_OF_RANGE,edge.index); }
                        if (unit_at(input,edge.index).object==0u64) { return failure(ENTRY_PROVIDER,edge.index); }
                        u64 state=word(work.states,edge.index);
                        if (state==1u64) { return failure(DEPENDENCY_CYCLE,edge.index); }
                        if (state==0u64) {
                            put_word(work.states,edge.index,1u64);
                            put_visit(work.visits,depth,Visit(edge.index,0u64));
                            depth+=1u64;
                        }
                    }
                }
            }
        }
        root+=1u64;
    }
    return GraphPlan(address(),0u64,0u64,0u64);
}

bool append_id(inout Providers providers,handle heap,u64 id) on cpu {
    if (providers.count==providers.capacity) {
        u64 capacity=providers.capacity==0u64 ? 16u64 : providers.capacity*2u64;
        address data=providers.capacity==0u64 ? HeapAlloc(heap,0,capacity*8u64) :
            HeapReAlloc(heap,0,providers.ids,capacity*8u64);
        if (address_bits(data)==0u64) { return false; }
        providers.ids=data;
        providers.capacity=capacity;
    }
    put_word(providers.ids,providers.count,id);
    providers.count+=1u64;
    return true;
}

extern "C" bool external_before(address left,address right,address context) on cpu {
    GraphInput input=unsafe_position<GraphInput>(context);
    return edge_at(input,word(left,0u64)).index<edge_at(input,word(right,0u64)).index;
}

bool index_externals(GraphInput input,address order,u64 count,handle heap,inout Providers providers) on cpu {
    sort_rows<u64>(order,count,address_of(input),c_function(external_before));
    u64 first=0u64;
    while (first<count) {
        u64 index=word(order,first);
        u64 identity=edge_at(input,index).index;
        u64 id=providers.count;
        if (!append_id(providers,heap,identity)) { return false; }
        put_edge(input,index,GraphEdge(id,1u64));
        first+=1u64;
        while (first<count) {
            index=word(order,first);
            if (edge_at(input,index).index!=identity) { break; }
            put_edge(input,index,GraphEdge(id,1u64));
            first+=1u64;
        }
    }
    return true;
}

bool append_provider(inout Providers providers,handle heap,address seen,u64 stamp,u64 id) on cpu {
    if (word(seen,id)==stamp) { return true; }
    if (!append_id(providers,heap,id)) { return false; }
    put_word(seen,id,stamp);
    return true;
}

bool expand_providers(GraphInput input,GraphOutput output,Scratch work,handle heap,
    inout Providers providers) on cpu {
    u64 ordered=0u64;
    while (ordered<input.count) {
        u64 index=word(work.order,ordered);
        u64 stamp=index+1u64;
        GraphRange direct=unit_at(input,index).dependencies;
        u64 start=providers.count;
        u64 at=0u64;
        while (at<direct.count) {
            GraphEdge edge=edge_at(input,direct.start+at);
            u64 external=u64(edge.external!=0u64);
            u64 id=edge.index+external*input.count;
            u64 delivered=u64(word(work.seen,id)==stamp);
            GraphRange inherited=GraphRange(0u64,0u64);
            if ((external|delivered)==0u64) {
                inherited=range_at(output.providers,id);
            }
            // A delivered provider needs neither its range nor another append.
            u64 child=delivered;
            while (child<=inherited.count) {
                u64 provider=child<inherited.count ? word(providers.ids,inherited.start+child) : id;
                if (!append_provider(providers,heap,work.seen,stamp,provider)) { return false; }
                child+=1u64;
            }
            at+=1u64;
        }
        put_range(output.providers,index,GraphRange(start,providers.count-start));
        ordered+=1u64;
    }
    return true;
}

void compact_direct_dependencies(GraphInput input, GraphOutput output, Scratch work) on cpu {
    u64 index = 0u64;
    while (index < input.count) {
        // Provider expansion used stamps 1..count. This pass visits each edge once.
        u64 stamp = input.count + index + 1u64;
        GraphRange direct = unit_at(input, index).dependencies;
        u64 count = 0u64;
        u64 at = 0u64;
        while (at < direct.count) {
            GraphEdge edge = edge_at(input, direct.start + at);
            if (edge.external == 0u64 && word(work.seen, edge.index) != stamp) {
                put_word(work.seen, edge.index, stamp);
                put_word(work.states, edge.index, word(work.states, edge.index) + 1u64);
                put_edge(input,direct.start+count,edge);
                count += 1u64;
            }
            at += 1u64;
        }
        put_word(output.waiting, index, count);
        index += 1u64;
    }
}

extern "C" GraphPlan plan(GraphSource source,GraphOutput output) on cpu
    requires nonzero(source.row) requires nonzero(source.edge) {
    handle heap=GetProcessHeap();
    u64 stride=source.count*8u64;
    u64 unit_bytes=source.count*size_of<GraphUnit>();
    u64 edge_bytes=source.edge_count*size_of<GraphEdge>();
    address memory=HeapAlloc(heap,8,stride*4u64+unit_bytes+edge_bytes);
    if (address_bits(memory)==0u64) { return failure(ERROR_NOT_ENOUGH_MEMORY,0u64); }
    GraphInput input=GraphInput(offset(memory,stride*4u64),offset(memory,stride*4u64+unit_bytes),source.count);
    // Only the final follower write needs this output buffer. Before then it
    // provides the ordering workspace for the actual external edges.
    u64 external_edges=read_dependencies(source,input,output.followers);
    Scratch work=Scratch(memory,offset(memory,stride),offset(memory,stride*2u64),address());
    GraphPlan order=dependency_order(input,work);
    if (order.status!=0u64) {
        HeapFree(heap,0,memory);
        return order;
    }

    Providers providers=Providers(address(),0u64,0u64);
    if (!index_externals(input,output.followers,external_edges,heap,providers)) {
        HeapFree(heap,0,memory);
        release_plan(providers.ids);
        return failure(ERROR_NOT_ENOUGH_MEMORY,0u64);
    }
    work.seen=HeapAlloc(heap,8,stride+providers.count*8u64);
    bool expanded=address_bits(work.seen)!=0u64 && expand_providers(input,output,work,heap,providers);
    if (!expanded) {
        release_plan(work.seen);
        HeapFree(heap,0,memory);
        release_plan(providers.ids);
        return failure(ERROR_NOT_ENOUGH_MEMORY,0u64);
    }

    // The traversal's state words now serve as incoming counts and write cursors.
    u64 index=0u64;
    while (index<input.count) { put_word(work.states,index,0u64); index+=1u64; }
    compact_direct_dependencies(input, output, work);
    u64 start=0u64;
    index=0u64;
    while (index<input.count) {
        u64 count=word(work.states,index);
        put_range(output.following,index,GraphRange(start,count));
        put_word(work.states,index,start);
        start+=count;
        index+=1u64;
    }
    index = 0u64;
    while (index < input.count) {
        GraphRange direct = unit_at(input, index).dependencies;
        u64 count = word(output.waiting, index);
        u64 at = 0u64;
        while (at < count) {
            u64 provider = edge_at(input, direct.start + at).index;
            u64 cursor = word(work.states, provider);
            put_word(output.followers, cursor, index);
            put_word(work.states, provider, cursor + 1u64);
            at += 1u64;
        }
        index += 1u64;
    }
    HeapFree(heap,0,work.seen);
    HeapFree(heap,0,memory);
    return GraphPlan(providers.ids,providers.count,0u64,0u64);
}

extern "C" void release_plan(address ids) on cpu {
    if (address_bits(ids)!=0u64) { HeapFree(GetProcessHeap(),0,ids); }
}
