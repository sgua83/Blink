module bootstrap.generic_bindings;
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";

struct BindingRow { NameBytes name; u64 value; u32 kind; u32 wrapped; };
struct BindingTable { BindingView view; u64 capacity; };

address at(address base, u64 offset) on cpu {
    return unsafe_address(address_bits(base) + offset);
}

address row_at(BindingView table, u64 index) on cpu {
    return at(table.rows, index * size_of<BindingRow>());
}

NameBytes binding_text(StorageView storage, Text text) on cpu {
    address bytes = text.kind == 0 ? storage.source : storage.text;
    return NameBytes(at(bytes, text.range.start), text.range.end - text.range.start);
}

u64 lower_bound(BindingView table, NameBytes name, u32 kind) on cpu {
    u64 begin = 0u64;
    u64 end = table.count;
    while (begin < end) {
        u64 middle = begin + (end - begin) / 2u64;
        BindingRow row = unsafe_position<BindingRow>(row_at(table, middle));
        if (row.kind < kind || (row.kind == kind && compare_name(row.name, name) < 0)) {
            begin = middle + 1u64;
        } else {
            end = middle;
        }
    }
    return begin;
}

bool matches(BindingView table, u64 index, NameBytes name, u32 kind) on cpu {
    if (index == table.count) { return false; }
    BindingRow row = unsafe_position<BindingRow>(row_at(table, index));
    return row.kind == kind && compare_name(row.name, name) == 0;
}

bool insert(address table, u64 index, BindingRow entry) on cpu {
    BindingTable storage = unsafe_position<BindingTable>(table);
    handle heap = GetProcessHeap();
    if (storage.view.count == storage.capacity) {
        u64 max_capacity = 9223372036854775807u64 / size_of<BindingRow>();
        if (storage.capacity == max_capacity) { return false; }
        u64 capacity = storage.capacity <= max_capacity / 2u64 ? storage.capacity * 2u64 : max_capacity;
        address rows;
        if (storage.capacity == 0u64) {
            capacity = 4u64;
            rows = HeapAlloc(heap, 0, capacity * size_of<BindingRow>());
        } else {
            rows = HeapReAlloc(heap, 0, storage.view.rows, capacity * size_of<BindingRow>());
        }
        if (address_bits(rows) == 0u64) { return false; }
        storage.view.rows = rows;
        storage.capacity = capacity;
        unsafe_position<BindingTable>(table) = storage;
    }
    address bytes = HeapAlloc(heap, 0, entry.name.length);
    if (address_bits(bytes) == 0u64) { return false; }
    u64 i = 0u64;
    while (i < entry.name.length) {
        unsafe_position<u8>(at(bytes, i)) = unsafe_position<u8>(at(entry.name.bytes, i));
        i += 1u64;
    }
    BindingView view = storage.view;
    i = view.count;
    while (i > index) {
        unsafe_position<BindingRow>(row_at(view, i)) = unsafe_position<BindingRow>(row_at(view, i - 1u64));
        i -= 1u64;
    }
    unsafe_position<BindingRow>(row_at(view, index)) =
        BindingRow(NameBytes(bytes, entry.name.length), entry.value, entry.kind, entry.wrapped);
    unsafe_position<BindingTable>(table).view.count += 1u64;
    return true;
}

extern "C" bool bind_type(address table, NameBytes name, address type_value, u32 wrapped,
    cfn<void,address> release_type) on cpu requires nonzero(release_type) {
    BindingView view = unsafe_position<BindingTable>(table).view;
    u64 index = lower_bound(view, name, 1);
    u64 value = address_bits(type_value);
    if (matches(view, index, name, 1)) {
        address row = row_at(view, index);
        release_type(unsafe_address(unsafe_position<BindingRow>(row).value));
        unsafe_position<BindingRow>(row).value = value;
        unsafe_position<BindingRow>(row).wrapped = wrapped;
        return true;
    }
    return insert(table, index, BindingRow(name, value, 1, wrapped));
}

extern "C" bool bind_value(address table, NameBytes name, u32 value) on cpu {
    BindingView view = unsafe_position<BindingTable>(table).view;
    u64 index = lower_bound(view, name, 2);
    if (matches(view, index, name, 2)) {
        unsafe_position<BindingRow>(row_at(view, index)).value = u64(value);
        return true;
    }
    return insert(table, index, BindingRow(name, u64(value), 2, 0));
}

extern "C" address bound_type(BindingView table, NameBytes name) on cpu {
    u64 index = lower_bound(table, name, 1);
    if (matches(table, index, name, 1)) {
        return unsafe_address(unsafe_position<BindingRow>(row_at(table, index)).value);
    }
    return unsafe_address(0u64);
}

extern "C" Optional<u32> bound_value(BindingView table, NameBytes name) on cpu {
    u64 index = lower_bound(table, name, 2);
    if (matches(table, index, name, 2)) {
        return Optional<u32>(1, u32(unsafe_position<BindingRow>(row_at(table, index)).value));
    }
    return Optional<u32>(0, 0);
}

extern "C" BindingEntry entry_at(BindingView table, u64 index) on cpu {
    BindingRow row = unsafe_position<BindingRow>(row_at(table, index));
    address type_value = unsafe_address(row.kind == 1 ? row.value : 0u64);
    return BindingEntry(row.name, type_value, row.kind == 2 ? u32(row.value) : 0, row.kind);
}

u32 use_kind(BindingView table, NameBytes name, u32 kind) on cpu {
    u64 index = lower_bound(table, name, kind);
    if (matches(table, index, name, kind)) {
        return 1 + unsafe_position<BindingRow>(row_at(table, index)).wrapped;
    }
    return 0;
}

extern "C" void release(BindingView table, cfn<void,address> release_type) on cpu requires nonzero(release_type) {
    handle heap = GetProcessHeap();
    u64 index = 0u64;
    while (index < table.count) {
        BindingRow row = unsafe_position<BindingRow>(row_at(table, index));
        if (row.kind == 1) { release_type(unsafe_address(row.value)); }
        HeapFree(heap, 0, row.name.bytes);
        index += 1u64;
    }
    HeapFree(heap, 0, table.rows);
}
