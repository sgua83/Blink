module bootstrap.declarations;
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";

struct Node { Entry entry; u64 hash; };

address at(address base, u64 offset) on cpu {
    return unsafe_address(address_bits(base) + offset);
}
address slot_at(address slots, u64 index) on cpu {
    return at(slots, index * size_of<address>());
}
bool present(address value) on cpu { return address_bits(value) != 0u64; }
NameBytes empty() on cpu { return NameBytes(address(), 0u64); }
u8 byte_at(NameBytes name, u64 index) on cpu {
    return unsafe_position<u8>(at(name.bytes, index));
}
u64 mix(u64 hash, u8 byte) on cpu { return (hash ^ u64(byte)) * 1099511628211u64; }
u64 hash_bytes(u64 seed, NameBytes text) on cpu {
    u64 hash = seed;
    u64 index = 0u64;
    while (index < text.length) {
        hash = mix(hash, byte_at(text, index));
        index += 1u64;
    }
    return hash;
}
u64 hash_name(NameBytes prefix, NameBytes name) on cpu {
    u64 hash = 14695981039346656037u64;
    if (present(prefix.bytes)) { hash = mix(mix(hash_bytes(hash, prefix), u8(58)), u8(58)); }
    return hash_bytes(hash, name);
}

// Read the exact qualified key as three spans while descending the same tree.
i32 compare_qualified(NameBytes stored, NameBytes prefix, NameBytes name) on cpu {
    if (!present(prefix.bytes)) { return compare_name(stored, name); }
    u64 start = prefix.length + 2u64;
    u64 length = start + name.length;
    u64 i = 0u64;
    while (i < length && i < stored.length) {
        u8 left = byte_at(stored, i);
        u8 right = i < prefix.length ? byte_at(prefix, i) : i < start ? u8(58) : byte_at(name, i - start);
        if (left != right) { return left < right ? i32(-1) : i32(1); }
        i += 1u64;
    }
    if (stored.length == length) { return i32(0); }
    return stored.length < length ? i32(-1) : i32(1);
}
address seek(address slots, u64 capacity, u64 hash, NameBytes prefix, NameBytes name) on cpu {
    u64 mask = capacity - 1u64;
    u64 index = hash & mask;
    while (true) {
        address slot = slot_at(slots, index);
        address node = unsafe_position<address>(slot);
        if (!present(node)) { return slot; }
        Node row = unsafe_position<Node>(node);
        if (row.hash == hash && compare_qualified(row.entry.name, prefix, name) == i32(0)) { return slot; }
        index = (index + 1u64) & mask;
    }
}

bool grow(address catalog) on cpu {
    Catalog storage = unsafe_position<Catalog>(catalog);
    if (storage.capacity > 9223372036854775807u64 / size_of<address>() / 2u64) { return false; }
    u64 capacity = storage.capacity == 0u64 ? 8u64 : storage.capacity * 2u64;
    handle heap = GetProcessHeap();
    address slots = HeapAlloc(heap, 8, capacity * size_of<address>());
    if (!present(slots)) { return false; }
    u64 index = 0u64;
    while (index < storage.capacity) {
        address node = unsafe_position<address>(slot_at(storage.slots, index));
        if (present(node)) {
            Node row = unsafe_position<Node>(node);
            address target = seek(slots, capacity, row.hash, empty(), row.entry.name);
            unsafe_position<address>(target) = node;
        }
        index += 1u64;
    }
    if (present(storage.slots)) { HeapFree(heap, 0, storage.slots); }
    storage.slots = slots;
    storage.capacity = capacity;
    unsafe_position<Catalog>(catalog) = storage;
    return true;
}

address make_node(NameBytes name, address value, u64 hash) on cpu {
    address node = HeapAlloc(GetProcessHeap(), 0, size_of<Node>() + name.length);
    if (!present(node)) { return node; }
    address bytes = at(node, size_of<Node>());
    u64 index = 0u64;
    while (index < name.length) {
        unsafe_position<u8>(at(bytes, index)) = byte_at(name, index);
        index += 1u64;
    }
    unsafe_position<Node>(node) = Node(Entry(NameBytes(bytes, name.length), value), hash);
    return node;
}
extern "C" Insertion insert(address catalog, NameBytes name, address value) on cpu {
    Catalog storage = unsafe_position<Catalog>(catalog);
    u64 hash = hash_name(empty(), name);
    address slot = address();
    if (storage.capacity != 0u64) {
        slot = seek(storage.slots, storage.capacity, hash, empty(), name);
        address node = unsafe_position<address>(slot);
        if (present(node)) {
            address old = unsafe_position<Node>(node).entry.value;
            unsafe_position<Node>(node).entry.value = value;
            return Insertion(old, 1);
        }
    }
    if (storage.count >= storage.capacity - storage.capacity / 4u64) {
        if (!grow(catalog)) { return Insertion(address(), 0); }
        storage = unsafe_position<Catalog>(catalog);
        slot = seek(storage.slots, storage.capacity, hash, empty(), name);
    }
    address node = make_node(name, value, hash);
    if (!present(node)) { return Insertion(address(), 0); }
    unsafe_position<address>(slot) = node;
    unsafe_position<Catalog>(catalog).count += 1u64;
    return Insertion(address(), 1);
}

address find_qualified(address catalog, NameBytes prefix, NameBytes name) on cpu {
    Catalog storage = unsafe_position<Catalog>(catalog);
    if (storage.capacity == 0u64) { return address(); }
    address slot = seek(storage.slots, storage.capacity, hash_name(prefix, name), prefix, name);
    return unsafe_position<address>(slot);
}
extern "C" address find(address catalog, NameBytes name) on cpu {
    return find_qualified(catalog, empty(), name);
}
extern "C" Entry entry(address node) on cpu { return unsafe_position<Node>(node).entry; }

address ordered_node(address position) on cpu {
    return unsafe_address(unsafe_position<u64>(position));
}
extern "C" bool declaration_before(address first, address second, address context) on cpu {
    return compare_name(entry(ordered_node(first)).name, entry(ordered_node(second)).name) < i32(0);
}
extern "C" bool seal(address catalog) on cpu {
    Catalog storage = unsafe_position<Catalog>(catalog);
    if (storage.count == 0u64) { return true; }
    address ordered = HeapAlloc(GetProcessHeap(), 0, storage.count * size_of<u64>());
    if (!present(ordered)) { return false; }
    u64 source = 0u64;
    u64 target = 0u64;
    while (source < storage.capacity) {
        address node = unsafe_position<address>(slot_at(storage.slots, source));
        if (present(node)) {
            unsafe_position<u64>(at(ordered, target * size_of<u64>())) = address_bits(node);
            target += 1u64;
        }
        source += 1u64;
    }
    sort_rows<u64>(ordered, storage.count, address(), c_function(declaration_before));
    unsafe_position<Catalog>(catalog).ordered = ordered;
    return true;
}
extern "C" Entry ordered_entry(address ordered, u64 index) on cpu {
    return entry(ordered_node(at(ordered, index * size_of<u64>())));
}

bool directly_imported(NameBytes owner, address imports, u64 count,
    cfn<NameBytes,address,u64> import_name) on cpu requires nonzero(import_name) {
    u64 i = 0u64;
    while (i < count) {
        if (compare_name(import_name(imports, i), owner) == i32(0)) { return true; }
        i += 1u64;
    }
    return false;
}
extern "C" Resolution resolve(address catalog, NameBytes name, NameBytes module, address imports,
    u64 count, cfn<NameBytes,address,u64> import_name, cfn<u32,address> visible) on cpu
    requires nonzero(import_name) requires nonzero(visible) {
    u64 separator = qualification_separator(name);
    if (separator < name.length) {
        address qualified = find(catalog, name);
        NameBytes owner = NameBytes(name.bytes, separator);
        if (present(qualified)) {
            Entry hit = entry(qualified);
            if (compare_name(owner, module) == i32(0)
                || (visible(hit.value) != 0 && directly_imported(owner, imports, count, import_name))) {
                return Resolution(hit.name, 1u64);
            }
        }
    }
    address local = find_qualified(catalog, module, name);
    if (present(local)) { return Resolution(entry(local).name, 1u64); }

    address found = address();
    u64 i = 0u64;
    while (i < count) {
        address current = find_qualified(catalog, import_name(imports, i), name);
        if (present(current) && visible(entry(current).value) != 0) {
            if (present(found) && address_bits(found) != address_bits(current)) { return Resolution(empty(), 2u64); }
            found = current;
        }
        i += 1u64;
    }
    return present(found) ? Resolution(entry(found).name, 1u64) : Resolution(empty(), 0u64);
}
extern "C" void release(address catalog, cfn<void,address> drop_value) on cpu requires nonzero(drop_value) {
    Catalog storage = unsafe_position<Catalog>(catalog);
    handle heap = GetProcessHeap();
    u64 index = 0u64;
    while (index < storage.capacity) {
        address node = unsafe_position<address>(slot_at(storage.slots, index));
        if (present(node)) {
            drop_value(entry(node).value);
            HeapFree(heap, 0, node);
        }
        index += 1u64;
    }
    if (present(storage.ordered)) { HeapFree(heap, 0, storage.ordered); }
    if (present(storage.slots)) { HeapFree(heap, 0, storage.slots); }
}
