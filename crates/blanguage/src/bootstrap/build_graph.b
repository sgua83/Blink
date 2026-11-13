module bootstrap.build_graph;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";

const u64 OUT_OF_RANGE = 1u64;
const u64 DEPENDENCY_CYCLE = 2u64;
const u64 ENTRY_PROVIDER = 3u64;
const u64 ERROR_NOT_ENOUGH_MEMORY = 8u64;

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
GraphRange range_at(address rows,u64 index) on cpu {
    return unsafe_position<GraphRange>(offset(rows,index*size_of<GraphRange>()));
}
void put_range(address rows,u64 index,GraphRange range) on cpu {
    unsafe_position<GraphRange>(offset(rows,index*size_of<GraphRange>()))=range;
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

bool append_provider(inout Providers providers,handle heap,address seen,u64 stamp,u64 id) on cpu {
    if (word(seen,id)==stamp) { return true; }
    if (providers.count==providers.capacity) {
        u64 capacity=providers.capacity==0u64 ? 16u64 : providers.capacity*2u64;
        address data=providers.capacity==0u64 ? HeapAlloc(heap,0,capacity*8u64) :
            HeapReAlloc(heap,0,providers.ids,capacity*8u64);
        if (address_bits(data)==0u64) { return false; }
        providers.ids=data;
        providers.capacity=capacity;
    }
    put_word(seen,id,stamp);
    put_word(providers.ids,providers.count,id);
    providers.count+=1u64;
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
            u64 id=input.count+edge.index;
            GraphRange inherited=GraphRange(0u64,0u64);
            if (edge.external==0u64) {
                id=edge.index;
                inherited=range_at(output.providers,id);
            }
            // Once a provider is present, its inherited range is already present.
            u64 child=u64(word(work.seen,id)==stamp)*inherited.count;
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
                unsafe_position<GraphEdge>(offset(input.edges, (direct.start + count) * size_of<GraphEdge>())) = edge;
                count += 1u64;
            }
            at += 1u64;
        }
        put_word(output.waiting, index, count);
        index += 1u64;
    }
}

extern "C" GraphPlan plan(GraphInput input,GraphOutput output) on cpu {
    handle heap=GetProcessHeap();
    u64 stride=input.count*8u64;
    address memory=HeapAlloc(heap,8,stride*5u64+input.external_count*8u64);
    if (address_bits(memory)==0u64) { return failure(ERROR_NOT_ENOUGH_MEMORY,0u64); }
    Scratch work=Scratch(memory,offset(memory,stride),offset(memory,stride*2u64),offset(memory,stride*4u64));
    GraphPlan order=dependency_order(input,work);
    if (order.status!=0u64) {
        HeapFree(heap,0,memory);
        return order;
    }

    Providers providers=Providers(address(),0u64,0u64);
    if (!expand_providers(input,output,work,heap,providers)) {
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
    HeapFree(heap,0,memory);
    return GraphPlan(providers.ids,providers.count,0u64,0u64);
}

extern "C" void release_plan(address ids) on cpu {
    if (address_bits(ids)!=0u64) { HeapFree(GetProcessHeap(),0,ids); }
}
