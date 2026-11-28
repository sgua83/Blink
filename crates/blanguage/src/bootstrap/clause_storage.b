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
