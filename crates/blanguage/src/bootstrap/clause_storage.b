module bootstrap.clause_storage;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" never abort() on cpu from "ucrtbase.dll";

struct ClauseHeader { u64 strong; u64 identities; };

address header_at(address payload) on cpu {
    return unsafe_address(address_bits(payload) - size_of<ClauseHeader>());
}

struct ClausePair { address first; address second; };
struct ClauseChildren { address first; address second; address third; };

address payload_fields(address payload) on cpu {
    return unsafe_address(address_bits(payload) + size_of<StoredClause>());
}

SourceNode clause_source(address clause) on cpu {
    address payload = unsafe_position<address>(clause);
    StoredClause node = unsafe_position<StoredClause>(payload);
    address fields = payload_fields(payload);
    if (node.kind == StoredClauseKind::Impossible) {
        return SourceNode(0,address(),address(),address(),0,0,address());
    }
    if (node.kind == StoredClauseKind::Facts) {
        u32 empty = u32(unsafe_position<OrderedTree>(fields).count == 0u64);
        return SourceNode(1,fields,address(),address(),0,empty,address());
    }
    address left = address_of(unsafe_position<ClausePair>(fields).first);
    address right = address_of(unsafe_position<ClausePair>(fields).second);
    if (node.kind == StoredClauseKind::Branch) {
        address cases = payload_fields(unsafe_position<address>(right));
        return SourceNode(4,left,address_of(unsafe_position<ClausePair>(cases).second),
                          address_of(unsafe_position<ClausePair>(cases).first),node.truth,0,right);
    }
    return SourceNode(u32(node.kind),left,right,address(),0,0,address());
}

address allocate_clause(StoredClauseKind kind,u32 truth,u64 bytes) on cpu {
    address header = HeapAlloc(GetProcessHeap(),0,size_of<ClauseHeader>() + size_of<StoredClause>() + bytes);
    if (header == address()) { abort(); }
    // The strong group holds one identity reference until its payload is destroyed.
    unsafe_position<ClauseHeader>(header) = ClauseHeader(1u64,1u64);
    address payload = unsafe_address(address_bits(header) + size_of<ClauseHeader>());
    unsafe_position<StoredClause>(payload) = StoredClause(kind,truth);
    return payload;
}

extern "C" address clause_impossible() on cpu {
    return allocate_clause(StoredClauseKind::Impossible,0,0u64);
}

extern "C" address clause_facts(address facts) on cpu {
    address payload = allocate_clause(StoredClauseKind::Facts,0,size_of<OrderedTree>());
    unsafe_position<OrderedTree>(payload_fields(payload)) = unsafe_position<OrderedTree>(facts);
    return payload;
}

extern "C" address clause_pair(StoredClauseKind kind,u32 truth,address first,address second) on cpu {
    address payload = allocate_clause(kind,truth,size_of<ClausePair>());
    unsafe_position<ClausePair>(payload_fields(payload)) = ClausePair(first,second);
    return payload;
}

void retain_count(address counter) on cpu {
    u64 previous = atomic_add(unsafe_position<atomic<u64>>(counter),1u64,relaxed);
    if (previous > 9223372036854775807u64) { abort(); }
}

address strong_at(address payload) on cpu {
    return address_of(unsafe_position<ClauseHeader>(header_at(payload)).strong);
}

address identities_at(address payload) on cpu {
    return address_of(unsafe_position<ClauseHeader>(header_at(payload)).identities);
}

extern "C" void clause_retain(address payload) on cpu {
    retain_count(strong_at(payload));
}

extern "C" void clause_retain_identity(address payload) on cpu {
    retain_count(identities_at(payload));
}

extern "C" void clause_release_identity(address payload) on cpu {
    if (atomic_sub(unsafe_position<atomic<u64>>(identities_at(payload)),1u64,acq_rel) == 1u64) {
        HeapFree(GetProcessHeap(),0,header_at(payload));
    }
}

struct PendingClauses { address records; u64 length; u64 capacity; };

address pending_slot(address records,u64 index) on cpu {
    return unsafe_address(address_bits(records) + index * size_of<address>());
}

void push_clause(inout PendingClauses pending,address payload) on cpu {
    if (payload == address()) { return; }
    if (pending.length == pending.capacity) {
        u64 next = pending.capacity == 0u64 ? 8u64 : pending.capacity * 2u64;
        address records = pending.records == address()
            ? HeapAlloc(GetProcessHeap(),0,next * size_of<address>())
            : HeapReAlloc(GetProcessHeap(),0,pending.records,next * size_of<address>());
        if (records == address()) { abort(); }
        pending.records = records;
        pending.capacity = next;
    }
    unsafe_position<address>(pending_slot(pending.records,pending.length)) = payload;
    pending.length += 1u64;
}

address pop_clause(inout PendingClauses pending) on cpu {
    if (pending.length == 0u64) { return address(); }
    pending.length -= 1u64;
    return unsafe_position<address>(pending_slot(pending.records,pending.length));
}

bool release_strong(address payload) on cpu {
    return atomic_sub(unsafe_position<atomic<u64>>(strong_at(payload)),1u64,acq_rel) == 1u64;
}

ClauseChildren consume_fields(address payload) on cpu {
    StoredClause node = unsafe_position<StoredClause>(payload);
    if (node.kind == StoredClauseKind::Impossible) {
        return ClauseChildren(address(),address(),address());
    }
    address fields = payload_fields(payload);
    if (node.kind == StoredClauseKind::Facts) {
        fact_tree_clear(fields);
        return ClauseChildren(address(),address(),address());
    }
    ClausePair pair = unsafe_position<ClausePair>(fields);
    if (node.kind != StoredClauseKind::Branch) {
        return ClauseChildren(pair.first,pair.second,address());
    }
    // Release the shared condition before following the branch's parent.
    if (release_strong(pair.second)) {
        ClausePair condition = unsafe_position<ClausePair>(payload_fields(pair.second));
        clause_release_identity(pair.second);
        return ClauseChildren(pair.first,condition.first,condition.second);
    }
    return ClauseChildren(pair.first,address(),address());
}

extern "C" void clause_release(address payload) on cpu {
    PendingClauses pending = PendingClauses(address(),0u64,0u64);
    address current = payload;
    while (current != address()) {
        if (release_strong(current)) {
            ClauseChildren children = consume_fields(current);
            clause_release_identity(current);
            push_clause(pending,children.second);
            push_clause(pending,children.third);
            current = children.first;
        } else {
            current = address();
        }
        if (current == address()) { current = pop_clause(pending); }
    }
    if (pending.records != address()) { HeapFree(GetProcessHeap(),0,pending.records); }
}

address retain_payload(address payload) on cpu {
    clause_retain(payload);
    return payload;
}

address pair_of(StoredClauseKind kind,u32 truth,address left,address right) on cpu {
    return clause_pair(kind,truth,retain_payload(left),retain_payload(right));
}

StoredClause clause_header(address payload) on cpu {
    return unsafe_position<StoredClause>(payload);
}

ClausePair clause_edges(address payload) on cpu {
    return unsafe_position<ClausePair>(payload_fields(payload));
}

address selected_case(address condition,u32 truth) on cpu {
    ClausePair cases=clause_edges(condition);
    return truth==0 ? cases.first : cases.second;
}

bool contains_conjunct(StoredClause source,address payload,address other) on cpu {
    if(source.kind!=StoredClauseKind::Both&&source.kind!=StoredClauseKind::Branch){return false;}
    ClausePair pair=clause_edges(payload);
    address right=source.kind==StoredClauseKind::Branch ? selected_case(pair.second,source.truth) : pair.second;
    return pair.first==other||right==other;
}

address complementary_parent(address left,address right) on cpu {
    StoredClause a=clause_header(left);
    StoredClause b=clause_header(right);
    if(a.kind==StoredClauseKind::Branch&&b.kind==StoredClauseKind::Branch&&a.truth!=b.truth){
        ClausePair x=clause_edges(left);
        ClausePair y=clause_edges(right);
        if(x.first==y.first&&x.second==y.second){return x.first;}
    }
    return address();
}

// The caller transfers this tree. A contradictory tree is released here.
extern "C" address checked_clause_facts(address facts,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    if(conflict(facts)!=0){
        fact_tree_clear(facts);
        return clause_impossible();
    }
    return clause_facts(facts);
}

address unite_facts(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    array<FactCursor,2> cursors=array<FactCursor,2>(fact_tree_cursor(left),fact_tree_cursor(right));
    OrderedTree facts=OrderedTree(address(),0u64,0u64,0u64);
    fact_tree_collect_merge(left,right,address_of(cursors[0]),0,address_of(facts));
    return checked_clause_facts(address_of(facts),conflict);
}

extern "C" address combine_clause_both(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    StoredClause a=clause_header(left);
    StoredClause b=clause_header(right);
    if(a.kind==StoredClauseKind::Impossible||b.kind==StoredClauseKind::Impossible){return clause_impossible();}
    if(a.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(left)).count==0u64){
        return retain_payload(right);
    }
    if(b.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(right)).count==0u64){
        return retain_payload(left);
    }
    if(a.kind==StoredClauseKind::Facts&&b.kind==StoredClauseKind::Facts){
        return unite_facts(payload_fields(left),payload_fields(right),conflict);
    }
    if(left==right||contains_conjunct(a,left,right)){return retain_payload(left);}
    if(contains_conjunct(b,right,left)){return retain_payload(right);}
    return pair_of(StoredClauseKind::Both,0,left,right);
}

extern "C" address combine_clause_either(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    address parent=complementary_parent(left,right);
    if(parent!=address()){return retain_payload(parent);}
    StoredClause a=clause_header(left);
    StoredClause b=clause_header(right);
    if(a.kind==StoredClauseKind::Either){
        ClausePair pair=clause_edges(left);
        parent=complementary_parent(pair.second,right);
        if(parent!=address()){return combine_clause_either(pair.first,parent,conflict);}
        parent=complementary_parent(pair.first,right);
        if(parent!=address()){return combine_clause_either(parent,pair.second,conflict);}
    }
    if(b.kind==StoredClauseKind::Either){
        ClausePair pair=clause_edges(right);
        parent=complementary_parent(left,pair.first);
        if(parent!=address()){return combine_clause_either(parent,pair.second,conflict);}
        parent=complementary_parent(left,pair.second);
        if(parent!=address()){return combine_clause_either(pair.first,parent,conflict);}
    }
    if(a.kind==StoredClauseKind::Impossible){return retain_payload(right);}
    if(b.kind==StoredClauseKind::Impossible){return retain_payload(left);}
    if(a.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(left)).count==0u64){
        return retain_payload(left);
    }
    if(b.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(right)).count==0u64){
        return retain_payload(right);
    }
    if(a.kind==StoredClauseKind::Facts&&b.kind==StoredClauseKind::Facts){
        address x=payload_fields(left);
        address y=payload_fields(right);
        if(fact_tree_relation(x,y,1)!=0){return retain_payload(left);}
        if(fact_tree_relation(y,x,1)!=0){return retain_payload(right);}
    }
    if(left==right){return retain_payload(left);}
    if(contains_conjunct(a,left,right)){return retain_payload(right);}
    if(contains_conjunct(b,right,left)){return retain_payload(left);}
    if(a.kind==StoredClauseKind::Both&&b.kind==StoredClauseKind::Both){
        ClausePair x=clause_edges(left);
        ClausePair y=clause_edges(right);
        address common=address();
        address first=address();
        address second=address();
        if(x.first==y.first){
            common=x.first;first=x.second;second=y.second;
        }else if(x.first==y.second){
            common=x.first;first=x.second;second=y.first;
        }else if(x.second==y.first){
            common=x.second;first=x.first;second=y.second;
        }else if(x.second==y.second){
            common=x.second;first=x.first;second=y.first;
        }
        if(common!=address()){
            address nested=combine_clause_either(first,second,conflict);
            address result=combine_clause_both(common,nested,conflict);
            clause_release(nested);
            return result;
        }
    }
    return pair_of(StoredClauseKind::Either,0,left,right);
}

extern "C" address combine_clause_branch(address parent,address condition,u32 truth) on cpu {
    if(clause_header(parent).kind==StoredClauseKind::Impossible||
       clause_header(selected_case(condition,truth)).kind==StoredClauseKind::Impossible){
        return clause_impossible();
    }
    return pair_of(StoredClauseKind::Branch,truth,parent,condition);
}
