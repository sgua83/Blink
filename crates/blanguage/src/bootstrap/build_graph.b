module bootstrap.build_graph;
import bootstrap.build_schedule;
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
struct GraphShape { u64 external_edges; u64 entries; };
struct Visit { u64 unit; u64 next; };
struct Scratch { address states; address order; address visits; address seen; };
struct Providers { address ids; u64 count; u64 capacity; };
struct PlanStatus { u64 status; u64 unit; };

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
struct EntryKey { CapturePath location; GraphRange providers; u64 unit; };
EntryKey entry_key(address entries,u64 index) on cpu {
    return unsafe_position<EntryKey>(offset(entries,index*size_of<EntryKey>()));
}
i32 compare_entries(EntryKey first,EntryKey second,address ids) on cpu {
    i32 path=compare_name(first.location.path,second.location.path);
    if (path!=i32(0)) { return path; }
    i32 root=compare_name(first.location.root,second.location.root);
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

void capture_owners(GraphPlan graph,CaptureSource source,address entries) on cpu
    requires nonzero(source.key) {
    address indices=graph.storage.captures;
    u64 index=0u64;
    while (index<graph.units) { put_word(indices,index,graph.units); index+=1u64; }
    address order=offset(entries,graph.entries*size_of<EntryKey>());
    u64 unit=0u64;
    u64 at=0u64;
    while (at<graph.entries) {
        address row=offset(entries,at*size_of<EntryKey>());
        if (source.key(source.context,unit,address_of(unsafe_position<EntryKey>(row).location))) {
            unsafe_position<EntryKey>(row).providers=range_at(graph.storage.providers,unit);
            unsafe_position<EntryKey>(row).unit=unit;
            put_word(order,at,at);
            at+=1u64;
        }
        unit+=1u64;
    }
    EntryOrder input=EntryOrder(entries,graph.ids);
    sort_rows<u64>(order,graph.entries,address_of(input),c_function(entry_before));
    u64 first=0u64;
    while (first<graph.entries) {
        EntryKey owner=entry_key(entries,word(order,first));
        u64 end=first+1u64;
        while (end<graph.entries) {
            EntryKey member=entry_key(entries,word(order,end));
            if (compare_entries(owner,member,graph.ids)!=i32(0)) { break; }
            put_word(indices,member.unit,owner.unit);
            end+=1u64;
        }
        if (end>first+1u64) { put_word(indices,owner.unit,owner.unit); }
        first=end;
    }
}

struct ProviderName { NameBytes name; u64 input; };

ProviderName provider_name_at(address rows,u64 index) on cpu {
    return unsafe_position<ProviderName>(offset(rows,index*size_of<ProviderName>()));
}

extern "C" bool capture_provider_before(address left,address right,address context) on cpu {
    ProviderName first=unsafe_position<ProviderName>(left);
    ProviderName second=unsafe_position<ProviderName>(right);
    i32 order=compare_name(first.name,second.name);
    return order==i32(0) ? first.input<second.input : order<i32(0);
}

extern "C" u64 lookup_capture_provider(ProviderNames providers,NameBytes name) on cpu {
    u64 first=0u64;
    u64 end=providers.count;
    while (first<end) {
        u64 middle=first+(end-first)/2u64;
        if (compare_name(provider_name_at(providers.rows,middle).name,name)<i32(0)) {
            first=middle+1u64;
        } else {
            end=middle;
        }
    }
    if (first<providers.count) {
        ProviderName selected=provider_name_at(providers.rows,first);
        if (compare_name(selected.name,name)==i32(0)) { return word(providers.ids,selected.input); }
    }
    return 18446744073709551615u64;
}

i32 capture_with_providers(GraphPlan graph,CaptureSource source,u64 unit,bool shared) on cpu
    requires nonzero(source.provider) requires nonzero(source.capture) {
    GraphRange providers=range_at(graph.storage.providers,unit);
    if (providers.count==0u64) {
        return source.capture(source.context,unit,shared,ProviderNames(address(),0u64,graph.ids));
    }
    handle heap=GetProcessHeap();
    address rows=HeapAlloc(heap,0,providers.count*size_of<ProviderName>());
    if (rows==address()) { return i32(ERROR_NOT_ENOUGH_MEMORY); }
    u64 at=0u64;
    while (at<providers.count) {
        u64 input=providers.start+at;
        NameBytes name=source.provider(source.context,word(graph.ids,input));
        unsafe_position<ProviderName>(offset(rows,at*size_of<ProviderName>()))=ProviderName(name,input);
        at+=1u64;
    }
    sort_rows<array<u64,3>>(rows,providers.count,address(),c_function(capture_provider_before));
    i32 status=source.capture(source.context,unit,shared,ProviderNames(rows,providers.count,graph.ids));
    HeapFree(heap,0,rows);
    return status;
}

extern "C" i32 capture_sources(address owner,CaptureSource source) on cpu
    requires nonzero(source.key) requires nonzero(source.provider) requires nonzero(source.capture) {
    GraphPlan graph=unsafe_position<GraphPlan>(owner);
    address indices=graph.storage.captures;
    bool grouped=graph.entries>1u64;
    if (grouped) {
        handle heap=GetProcessHeap();
        address entries=HeapAlloc(heap,0,graph.entries*(size_of<EntryKey>()+8u64));
        if (address_bits(entries)==0u64) { return i32(ERROR_NOT_ENOUGH_MEMORY); }
        capture_owners(graph,source,entries);
        HeapFree(heap,0,entries);
    }
    // The earliest original unit has filled its snapshot index before its
    // borrowers. Only owners and independent units read sources.
    u64 count=0u64;
    u64 unit=0u64;
    while (unit<graph.units) {
        u64 source_unit=grouped ? word(indices,unit) : graph.units;
        if (source_unit<unit) {
            put_word(indices,unit,word(indices,source_unit));
        } else {
            i32 status=capture_with_providers(graph,source,unit,source_unit==unit);
            if (status!=i32(0)) { return status; }
            put_word(indices,unit,count);
            count+=1u64;
        }
        unit+=1u64;
    }
    return i32(0);
}
Visit visit_at(address visits,u64 index) on cpu {
    return unsafe_position<Visit>(offset(visits,index*size_of<Visit>()));
}
void put_visit(address visits,u64 index,Visit visit) on cpu {
    unsafe_position<Visit>(offset(visits,index*size_of<Visit>()))=visit;
}

GraphPlan failure(u64 status,u64 unit) on cpu {
    GraphStorage empty=GraphStorage(address(),address(),address(),address(),address(),address(),address());
    return GraphPlan(address(),0u64,empty,0u64,0u64,status,unit);
}

void release_allocation(handle heap,address data) on cpu {
    if (data!=address()) { HeapFree(heap,0,data); }
}

GraphStorage allocate_storage(GraphSource source,handle heap) on cpu {
    u64 rows=source.count*size_of<GraphRange>();
    u64 words=source.count*size_of<u64>();
    u64 waiting=rows*2u64;
    u64 ready=waiting+words;
    u64 followers=ready+words;
    u64 captures=followers+source.edge_count*size_of<u64>();
    u64 counts=captures+words;
    address memory=HeapAlloc(heap,0,counts+size_of<ScheduleCounts>());
    if (memory==address()) {
        return GraphStorage(memory,memory,memory,memory,memory,memory,memory);
    }
    return GraphStorage(memory,offset(memory,rows),offset(memory,waiting),
        offset(memory,followers),offset(memory,ready),offset(memory,counts),offset(memory,captures));
}

GraphShape read_dependencies(GraphSource source,GraphInput input,address external_order) on cpu
    requires nonzero(source.row) requires nonzero(source.edge) {
    u64 unit=0u64;
    u64 at=0u64;
    u64 external_count=0u64;
    u64 entries=0u64;
    while (unit<source.count) {
        GraphRow row=source.row(source.units,unit);
        entries+=u64(row.object==0u64);
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
    return GraphShape(external_count,entries);
}

PlanStatus dependency_order(GraphInput input,Scratch work) on cpu {
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
                        if (edge.index>=input.count) { return PlanStatus(OUT_OF_RANGE,edge.index); }
                        if (unit_at(input,edge.index).object==0u64) { return PlanStatus(ENTRY_PROVIDER,edge.index); }
                        u64 state=word(work.states,edge.index);
                        if (state==1u64) { return PlanStatus(DEPENDENCY_CYCLE,edge.index); }
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
    return PlanStatus(0u64,0u64);
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

bool expand_providers(GraphInput input,GraphStorage output,Scratch work,handle heap,
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

u64 compact_direct_dependencies(GraphInput input, GraphStorage output, Scratch work) on cpu {
    u64 index = 0u64;
    u64 ready = 0u64;
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
        // Ascending unit indices form the scheduler's initial min-heap.
        if (count == 0u64) {
            put_word(output.ready, ready, index);
            ready += 1u64;
        }
        index += 1u64;
    }
    return ready;
}

void write_schedule(GraphInput input,GraphStorage output,Scratch work) on cpu {
    // The traversal's state words now serve as incoming counts and write cursors.
    u64 index=0u64;
    while (index<input.count) { put_word(work.states,index,0u64); index+=1u64; }
    u64 ready=compact_direct_dependencies(input, output, work);
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
    unsafe_position<ScheduleCounts>(output.counts)=ScheduleCounts(ready,input.count);
}

extern "C" GraphPlan plan(GraphSource source) on cpu
    requires nonzero(source.row) requires nonzero(source.edge) {
    handle heap=GetProcessHeap();
    GraphStorage output=allocate_storage(source,heap);
    if (output.providers==address()) { return failure(ERROR_NOT_ENOUGH_MEMORY,0u64); }

    u64 stride=source.count*8u64;
    u64 unit_bytes=source.count*size_of<GraphUnit>();
    u64 edge_bytes=source.edge_count*size_of<GraphEdge>();
    address memory=HeapAlloc(heap,8,stride*4u64+unit_bytes+edge_bytes);
    if (memory==address()) {
        HeapFree(heap,0,output.providers);
        return failure(ERROR_NOT_ENOUGH_MEMORY,0u64);
    }
    GraphInput input=GraphInput(offset(memory,stride*4u64),offset(memory,stride*4u64+unit_bytes),source.count);
    // The final follower table provides the ordering workspace until scheduling.
    GraphShape shape=read_dependencies(source,input,output.followers);
    Scratch work=Scratch(memory,offset(memory,stride),offset(memory,stride*2u64),address());
    PlanStatus order=dependency_order(input,work);
    if (order.status!=0u64) {
        HeapFree(heap,0,memory);
        HeapFree(heap,0,output.providers);
        return failure(order.status,order.unit);
    }

    Providers providers=Providers(address(),0u64,0u64);
    if (index_externals(input,output.followers,shape.external_edges,heap,providers)) {
        work.seen=HeapAlloc(heap,8,stride+providers.count*8u64);
        if (work.seen!=address() && expand_providers(input,output,work,heap,providers)) {
            write_schedule(input,output,work);
            HeapFree(heap,0,work.seen);
            HeapFree(heap,0,memory);
            return GraphPlan(providers.ids,providers.count,output,source.count,shape.entries,0u64,0u64);
        }
    }
    release_allocation(heap,work.seen);
    release_allocation(heap,providers.ids);
    HeapFree(heap,0,memory);
    HeapFree(heap,0,output.providers);
    return failure(ERROR_NOT_ENOUGH_MEMORY,0u64);
}

extern "C" void release_plan(address ids,address storage) on cpu {
    handle heap=GetProcessHeap();
    release_allocation(heap,ids);
    release_allocation(heap,storage);
}
