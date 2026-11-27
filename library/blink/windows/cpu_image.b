module blink.windows.cpu_image;
import blink.windows.coff;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 size) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, u64 size) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" u32 GetLastError() on cpu from "KERNEL32.dll";
extern "C" handle GetCurrentProcess() on cpu from "KERNEL32.dll";
extern "C" address VirtualAlloc(address base, u64 size, u32 kind, u32 protection) on cpu from "KERNEL32.dll";
extern "C" i32 VirtualFree(address base, u64 size, u32 kind) on cpu from "KERNEL32.dll";
extern "C" i32 VirtualProtect(address base, u64 size, u32 protection, write u32 old_protection) on cpu
    ensures initialized(old_protection, returned != 0) from "KERNEL32.dll";
extern "C" u64 VirtualQuery(address base, write MemoryInformation info, u64 size) on cpu
    ensures initialized(info, returned != 0u64) from "KERNEL32.dll";
extern "C" i32 FlushInstructionCache(handle process, address base, u64 size) on cpu from "KERNEL32.dll";
extern "C" u8 RtlAddFunctionTable(address table, u32 count, u64 base) on cpu from "KERNEL32.dll";
extern "C" u8 RtlDeleteFunctionTable(address table) on cpu from "KERNEL32.dll";
extern "C" i32 MultiByteToWideChar(u32 page, u32 flags, address input, i32 bytes,
    address output, i32 capacity) on cpu from "KERNEL32.dll";
extern "C" handle LoadLibraryW(address name) on cpu from "KERNEL32.dll";
extern "C" address GetProcAddress(handle library, address name) on cpu from "KERNEL32.dll";
extern "C" i32 FreeLibrary(handle library) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 size) on cpu from "KERNEL32.dll";

const u64 PAGE_SIZE = 4096u64;
const u64 U32_MAX = 4294967295u64;
const u64 I32_MAX = 2147483647u64;

struct RuntimeFunction { u32 begin; u32 end; u32 unwind; };
struct Rows { address data; u64 count; u64 capacity; };
struct ImageOwner { address base; Rows registered; Rows libraries; };
struct ImportSlot { ImageName name; u64 offset; };
struct Search { u64 index; bool found; };
struct BoundSymbol { address value; ImageError error; };
struct OpenLibrary { handle value; ImageError error; };
struct ImageLayout {
    u64 code_bytes;
    u64 size;
    u64 table_offset;
    address sections;
    Rows imports;
    Rows keys;
};
struct LayoutResult { ImageLayout layout; ImageError error; };

address offset_at(address start, u64 offset) on cpu {
    return unsafe_address(address_bits(start) + offset);
}

u64 aligned(u64 size, u64 alignment) on cpu {
    return (size + alignment - 1u64) & ~(alignment - 1u64);
}

ImageError failure(ImageFailure kind, u32 detail) on cpu {
    return ImageError(kind, detail, ImageName(ReadBytes(unsafe_address(0u64), 0u64), ReadBytes(unsafe_address(0u64), 0u64)));
}

ImageResult empty_result(ImageError error) on cpu {
    address empty = unsafe_address(0u64);
    return ImageResult(empty, empty, empty, empty, empty, error);
}

Rows empty_rows() on cpu { return Rows(unsafe_address(0u64), 0u64, 0u64); }

void free_bytes(address bytes) on cpu {
    if (address_bits(bytes) != 0u64) { HeapFree(GetProcessHeap(), 0, bytes); }
}

bool reserve_one(inout Rows rows, u64 stride) on cpu requires nonzero(stride) {
    if (rows.count < rows.capacity) { return true; }
    u64 capacity = rows.capacity == 0u64 ? 4u64 : rows.capacity * 2u64;
    address data = rows.capacity == 0u64
        ? HeapAlloc(GetProcessHeap(), 0, capacity * stride)
        : HeapReAlloc(GetProcessHeap(), 0, rows.data, capacity * stride);
    if (address_bits(data) == 0u64) { return false; }
    rows.data = data;
    rows.capacity = capacity;
    return true;
}

void shift_rows(Rows rows, u64 index, u64 stride) on cpu {
    if (index == rows.count) { return; }
    RtlMoveMemory(offset_at(rows.data, (index + 1u64) * stride), offset_at(rows.data, index * stride),
        (rows.count - index) * stride);
}

i32 compare_bytes(ReadBytes a, ReadBytes b) on cpu {
    u64 bytes = min(a.length, b.length);
    u64 index = 0u64;
    while (index < bytes) {
        u8 left = unsafe_position<u8>(offset_at(a.bytes, index));
        u8 right = unsafe_position<u8>(offset_at(b.bytes, index));
        if (left != right) { return left < right ? i32(-1) : i32(1); }
        index += 1u64;
    }
    return a.length == b.length ? i32(0) : (a.length < b.length ? i32(-1) : i32(1));
}

i32 compare_imports(ImageName a, ImageName b) on cpu {
    i32 order = compare_bytes(a.library, b.library);
    return order == 0 ? compare_bytes(a.symbol, b.symbol) : order;
}

Search find_import(Rows rows, ImageName name) on cpu {
    u64 low = 0u64;
    u64 high = rows.count;
    while (low < high) {
        u64 middle = low + (high - low) / 2u64;
        ImportSlot slot = unsafe_position<ImportSlot>(offset_at(rows.data, middle * size_of<ImportSlot>()));
        i32 order = compare_imports(slot.name, name);
        if (order == 0) { return Search(middle, true); }
        if (order < 0) { low = middle + 1u64; } else { high = middle; }
    }
    return Search(low, false);
}

bool insert_import(inout Rows rows, u64 index, ImageName name, u64 offset) on cpu {
    if (!reserve_one(rows, size_of<ImportSlot>())) { return false; }
    shift_rows(rows, index, size_of<ImportSlot>());
    unsafe_position<ImportSlot>(offset_at(rows.data, index * size_of<ImportSlot>())) = ImportSlot(name, offset);
    rows.count += 1u64;
    return true;
}

Search find_library(Rows keys, ReadBytes name) on cpu {
    u64 low = 0u64;
    u64 high = keys.count;
    while (low < high) {
        u64 middle = low + (high - low) / 2u64;
        ReadBytes key = unsafe_position<ReadBytes>(offset_at(keys.data, middle * size_of<ReadBytes>()));
        i32 order = compare_bytes(key, name);
        if (order == 0) { return Search(middle, true); }
        if (order < 0) { low = middle + 1u64; } else { high = middle; }
    }
    return Search(low, false);
}

extern "C" NativeLibraryOpen open_native_library(address name, u64 length) on cpu {
    handle none = handle();
    if (length > I32_MAX) { return NativeLibraryOpen(none, 87, 0); }
    // Every UTF-8 byte contributes at most one UTF-16 unit. The last unit starts zero.
    address wide = HeapAlloc(GetProcessHeap(), 8, (length + 1u64) * 2u64);
    if (address_bits(wide) == 0u64) { return NativeLibraryOpen(none, 8, 1); }
    if (length != 0u64 && MultiByteToWideChar(65001, 8, name, i32(length), wide, i32(length)) == 0) {
        u32 error = GetLastError();
        free_bytes(wide);
        return NativeLibraryOpen(none, error, 0);
    }
    handle library = LoadLibraryW(wide);
    u32 error = library == none ? GetLastError() : 0;
    free_bytes(wide);
    return NativeLibraryOpen(library, error, 0);
}

extern "C" NativeLibrarySymbol native_library_symbol(handle library, address nul_name) on cpu {
    address value = GetProcAddress(library, nul_name);
    u32 error = address_bits(value) == 0u64 ? GetLastError() : 0;
    return NativeLibrarySymbol(value, error);
}

extern "C" void release_native_library(handle library) on cpu {
    FreeLibrary(library);
}

OpenLibrary library_for(inout Rows keys, inout Rows libraries, ImageName name) on cpu {
    Search found = find_library(keys, name.library);
    if (found.found) {
        handle library = unsafe_position<handle>(offset_at(libraries.data, found.index * size_of<handle>()));
        return OpenLibrary(library, failure(ImageFailure::None, 0));
    }
    if (!reserve_one(keys, size_of<ReadBytes>()) || !reserve_one(libraries, size_of<handle>())) {
        return OpenLibrary(handle(), failure(ImageFailure::Heap, 8));
    }
    NativeLibraryOpen opened = open_native_library(name.library.bytes, name.library.length);
    if (opened.value == handle()) {
        ImageError error = opened.allocation_failed != 0
            ? failure(ImageFailure::Heap, opened.os_code)
            : ImageError(ImageFailure::Library, opened.os_code, name);
        return OpenLibrary(opened.value, error);
    }
    // The original DLL-name order also determines the final release order.
    shift_rows(keys, found.index, size_of<ReadBytes>());
    shift_rows(libraries, found.index, size_of<handle>());
    unsafe_position<ReadBytes>(offset_at(keys.data, found.index * size_of<ReadBytes>())) = name.library;
    unsafe_position<handle>(offset_at(libraries.data, found.index * size_of<handle>())) = opened.value;
    keys.count += 1u64;
    libraries.count += 1u64;
    return OpenLibrary(opened.value, failure(ImageFailure::None, 0));
}

BoundSymbol bind_external(ImageInput input, inout Rows keys, inout Rows libraries, ImageName name) on cpu
    requires nonzero(input.resource) {
    address empty = unsafe_address(0u64);
    if (compare_bytes(name.library, input.runtime_library) == 0) {
        address value = input.resource(input.context, name.symbol);
        if (address_bits(value) == 0u64) { return BoundSymbol(empty, ImageError(ImageFailure::Resource, 0, name)); }
        return BoundSymbol(value, failure(ImageFailure::None, 0));
    }
    OpenLibrary opened = library_for(keys, libraries, name);
    if (opened.error.kind != ImageFailure::None) { return BoundSymbol(empty, opened.error); }
    u64 index = 0u64;
    while (index < name.symbol.length) {
        if (unsafe_position<u8>(offset_at(name.symbol.bytes, index)) == u8(0)) {
            return BoundSymbol(empty, ImageError(ImageFailure::Nul, 0, name));
        }
        index += 1u64;
    }
    address text = HeapAlloc(GetProcessHeap(), 8, name.symbol.length + 1u64);
    if (address_bits(text) == 0u64) { return BoundSymbol(empty, failure(ImageFailure::Heap, 8)); }
    if (name.symbol.length != 0u64) { RtlMoveMemory(text, name.symbol.bytes, name.symbol.length); }
    NativeLibrarySymbol symbol = native_library_symbol(opened.value, text);
    free_bytes(text);
    if (address_bits(symbol.value) == 0u64) {
        return BoundSymbol(empty, ImageError(ImageFailure::Symbol, symbol.os_code, name));
    }
    return BoundSymbol(symbol.value, failure(ImageFailure::None, 0));
}

u64 section_base(ImageLayout layout, u64 index) on cpu {
    return unsafe_position<u64>(offset_at(layout.sections, index * 8u64));
}

void store_pointer(address field, address value) on cpu {
    u64 bits = address_bits(value);
    RtlMoveMemory(field, address_of(bits), 8u64);
}

address load_pointer(address field) on cpu {
    u64 bits = 0u64;
    // Use byte reads because the original machine-code field may be unaligned.
    u64 index = 0u64;
    while (index < 8u64) {
        bits |= u64(unsafe_position<u8>(offset_at(field, index))) << (index * 8u64);
        index += 1u64;
    }
    return unsafe_address(bits);
}

u64 code_unwind(u64 begin, ImageCode code) on cpu { return aligned(begin + code.bytes.length, 4u64); }

LayoutResult plan_image(ImageInput input) on cpu
    requires nonzero(input.code) requires nonzero(input.section) requires nonzero(input.section_patch) {
    ImageLayout layout = ImageLayout(0u64, 0u64, 0u64, unsafe_address(0u64), empty_rows(), empty_rows());
    if (input.code_count > U32_MAX) { return LayoutResult(layout, failure(ImageFailure::Range, 0)); }
    u64 cursor = 0u64;
    u64 code_index = 0u64;
    while (code_index < input.code_count) {
        ImageCode code = input.code(input.context, code_index);
        u64 begin = aligned(cursor, 16u64);
        u64 unwind = code_unwind(begin, code);
        if (unwind > U32_MAX || code.function_end > U32_MAX) {
            return LayoutResult(layout, failure(ImageFailure::Range, 0));
        }
        cursor = unwind + code.unwind.length;
        code_index += 1u64;
    }
    layout.code_bytes = cursor;
    if (input.section_count != 0u64) {
        layout.sections = HeapAlloc(GetProcessHeap(), 0, input.section_count * 8u64);
        if (address_bits(layout.sections) == 0u64) { return LayoutResult(layout, failure(ImageFailure::Heap, 8)); }
    }
    u64 section_index = 0u64;
    while (section_index < input.section_count) {
        ImageSection section = input.section(input.context, section_index);
        u32 field = (section.flags >> u32(20)) & u32(15);
        u64 alignment = max(PAGE_SIZE, 1u64 << u64(field == 0 ? 0 : field - 1));
        cursor = aligned(cursor, alignment);
        unsafe_position<u64>(offset_at(layout.sections, section_index * 8u64)) = cursor;
        cursor += section.bytes.length;
        u64 ordinal = 0u64;
        while (ordinal < section.patch_count) {
            SectionPatch patch = input.section_patch(input.context, section_index, ordinal);
            if (patch.external != 0) {
                Search found = find_import(layout.imports, patch.name);
                if (!found.found && !insert_import(layout.imports, found.index, patch.name, 0u64)) {
                    return LayoutResult(layout, failure(ImageFailure::Heap, 8));
                }
            }
            ordinal += 1u64;
        }
        section_index += 1u64;
    }
    if (input.has_module != 0) { cursor = aligned(cursor, PAGE_SIZE); }
    u64 slot_index = 0u64;
    while (slot_index < layout.imports.count) {
        address slot = offset_at(layout.imports.data, slot_index * size_of<ImportSlot>());
        unsafe_position<ImportSlot>(slot).offset = cursor;
        cursor += 8u64;
        slot_index += 1u64;
    }
    layout.table_offset = aligned(cursor, 4u64);
    layout.size = layout.table_offset + input.code_count * size_of<RuntimeFunction>();
    return LayoutResult(layout, failure(ImageFailure::None, 0));
}

void copy_code(ImageInput input, address base, address table) on cpu requires nonzero(input.code) {
    u64 cursor = 0u64;
    u64 index = 0u64;
    while (index < input.code_count) {
        ImageCode code = input.code(input.context, index);
        u64 begin = aligned(cursor, 16u64);
        u64 unwind = code_unwind(begin, code);
        if (code.bytes.length != 0u64) { RtlMoveMemory(offset_at(base, begin), code.bytes.bytes, code.bytes.length); }
        if (code.unwind.length != 0u64) { RtlMoveMemory(offset_at(base, unwind), code.unwind.bytes, code.unwind.length); }
        unsafe_position<RuntimeFunction>(offset_at(table, index * size_of<RuntimeFunction>())) =
            RuntimeFunction(u32(begin), u32(begin) + u32(code.function_end), u32(unwind));
        cursor = unwind + code.unwind.length;
        index += 1u64;
    }
}

address find_address(ImageInput input, ImageLayout layout, address base, address table, ReadBytes name) on cpu
    requires nonzero(input.symbol) {
    SymbolPosition position = input.symbol(input.context, name);
    if (position.target == ImageTarget::Section) {
        return offset_at(base, section_base(layout, position.index) + position.value);
    }
    if (position.target == ImageTarget::Root) {
        RuntimeFunction entry = unsafe_position<RuntimeFunction>(offset_at(table, position.index * size_of<RuntimeFunction>()));
        return offset_at(base, u64(entry.begin));
    }
    return unsafe_address(0u64);
}

ImageError bind_slots(ImageInput input, inout ImageLayout layout, inout ImageOwner owner) on cpu
    requires nonzero(input.resource) {
    u64 index = 0u64;
    while (index < layout.imports.count) {
        ImportSlot slot = unsafe_position<ImportSlot>(offset_at(layout.imports.data, index * size_of<ImportSlot>()));
        BoundSymbol symbol = bind_external(input, layout.keys, owner.libraries, slot.name);
        if (symbol.error.kind != ImageFailure::None) { return symbol.error; }
        store_pointer(offset_at(owner.base, slot.offset), symbol.value);
        index += 1u64;
    }
    return failure(ImageFailure::None, 0);
}

void sift_unwind(address entries, u64 root, u64 count) on cpu {
    u64 cursor = root;
    while (cursor < count / 2u64) {
        u64 child = cursor * 2u64 + 1u64;
        address child_address = offset_at(entries, child * size_of<RuntimeFunction>());
        RuntimeFunction next = unsafe_position<RuntimeFunction>(child_address);
        if (child + 1u64 < count) {
            address right_address = offset_at(entries, (child + 1u64) * size_of<RuntimeFunction>());
            RuntimeFunction right = unsafe_position<RuntimeFunction>(right_address);
            if (right.begin > next.begin) {
                child += 1u64;
                child_address = right_address;
                next = right;
            }
        }
        address current_address = offset_at(entries, cursor * size_of<RuntimeFunction>());
        RuntimeFunction current = unsafe_position<RuntimeFunction>(current_address);
        if (current.begin >= next.begin) { return; }
        unsafe_position<RuntimeFunction>(current_address) = next;
        unsafe_position<RuntimeFunction>(child_address) = current;
        cursor = child;
    }
}

void sort_unwind(address entries, u64 count) on cpu {
    u64 root = count / 2u64;
    while (root != 0u64) {
        root -= 1u64;
        sift_unwind(entries, root, count);
    }
    u64 end = count;
    while (end > 1u64) {
        end -= 1u64;
        address last_address = offset_at(entries, end * size_of<RuntimeFunction>());
        RuntimeFunction first = unsafe_position<RuntimeFunction>(entries);
        RuntimeFunction last = unsafe_position<RuntimeFunction>(last_address);
        unsafe_position<RuntimeFunction>(last_address) = first;
        unsafe_position<RuntimeFunction>(entries) = last;
        sift_unwind(entries, 0u64, end);
    }
}

ImageError patch_sections(ImageInput input, ImageLayout layout, address base) on cpu
    requires nonzero(input.section) requires nonzero(input.section_patch) {
    u64 index = 0u64;
    while (index < input.section_count) {
        ImageSection section = input.section(input.context, index);
        u64 begin = section_base(layout, index);
        address bytes = offset_at(base, begin);
        if (section.bytes.length != 0u64) { RtlMoveMemory(bytes, section.bytes.bytes, section.bytes.length); }
        u64 ordinal = 0u64;
        while (ordinal < section.patch_count) {
            SectionPatch patch = input.section_patch(input.context, index, ordinal);
            u64 target = 0u64;
            if (patch.external != 0) {
                Search found = find_import(layout.imports, patch.name);
                ImportSlot slot = unsafe_position<ImportSlot>(offset_at(layout.imports.data, found.index * size_of<ImportSlot>()));
                target = slot.offset;
            } else {
                target = section_base(layout, patch.section_index) + patch.section_value;
            }
            u32 status = apply_relocation(bytes, section.bytes.length, patch.offset, patch.kind,
                address_bits(base), begin + patch.offset, target);
            if (status != 0) {
                ImageFailure problem = status == 1 ? ImageFailure::Relocation
                    : (status == 2 ? ImageFailure::RelocationPosition : ImageFailure::RelocationValue);
                return failure(problem, patch.kind);
            }
            ordinal += 1u64;
        }
        if (section.unwind != 0) {
            u64 count = section.bytes.length / size_of<RuntimeFunction>();
            if (count > U32_MAX) { return failure(ImageFailure::Range, 0); }
            sort_unwind(bytes, count);
        }
        index += 1u64;
    }
    return failure(ImageFailure::None, 0);
}

ImageError patch_code(ImageInput input, inout ImageLayout layout, inout ImageOwner owner, address table) on cpu
    requires nonzero(input.code) requires nonzero(input.code_patch) requires nonzero(input.symbol)
    requires nonzero(input.resource) {
    u64 index = 0u64;
    while (index < input.code_count) {
        ImageCode code = input.code(input.context, index);
        RuntimeFunction row = unsafe_position<RuntimeFunction>(offset_at(table, index * size_of<RuntimeFunction>()));
        u64 ordinal = 0u64;
        while (ordinal < code.patch_count) {
            CodePatch patch = input.code_patch(input.context, index, ordinal);
            address value = unsafe_address(0u64);
            if (patch.target == ImageTarget::Root) {
                value = find_address(input, layout, owner.base, table, patch.name.symbol);
                if (address_bits(value) == 0u64) { return ImageError(ImageFailure::Function, 0, patch.name); }
            } else if (patch.target == ImageTarget::External) {
                Search found = find_import(layout.imports, patch.name);
                if (found.found) {
                    ImportSlot slot = unsafe_position<ImportSlot>(offset_at(layout.imports.data, found.index * size_of<ImportSlot>()));
                    value = load_pointer(offset_at(owner.base, slot.offset));
                } else {
                    BoundSymbol symbol = bind_external(input, layout.keys, owner.libraries, patch.name);
                    if (symbol.error.kind != ImageFailure::None) { return symbol.error; }
                    value = symbol.value;
                    if (!insert_import(layout.imports, found.index, patch.name, u64(row.begin) + patch.offset)) {
                        return failure(ImageFailure::Heap, 8);
                    }
                }
            } else {
                value = patch.service;
            }
            store_pointer(offset_at(owner.base, u64(row.begin) + patch.offset), value);
            ordinal += 1u64;
        }
        index += 1u64;
    }
    return failure(ImageFailure::None, 0);
}

u32 section_protection(u32 flags) on cpu {
    u32 access = flags & u32(0xe0000000);
    if (access == u32(0xe0000000) || access == u32(0xa0000000)) { return 0x40; }
    if (access == u32(0x60000000)) { return 0x20; }
    if (access == u32(0x20000000)) { return 0x10; }
    if (access == u32(0xc0000000) || access == u32(0x80000000)) { return 0x04; }
    if (access == u32(0x40000000)) { return 0x02; }
    return 0x01;
}

ImageError protect_pages(address base, u64 begin, u64 length, u32 protection) on cpu {
    if (length != 0u64) {
        u32 previous;
        if (VirtualProtect(offset_at(base, begin), length, protection, previous) == 0) {
            return failure(ImageFailure::Windows, GetLastError());
        }
    }
    return failure(ImageFailure::None, 0);
}

ImageError protect_image(ImageInput input, ImageLayout layout, address base) on cpu requires nonzero(input.section) {
    ImageError result = protect_pages(base, 0u64, layout.code_bytes, 0x20);
    if (result.kind != ImageFailure::None) { return result; }
    u64 end = aligned(layout.code_bytes, PAGE_SIZE);
    u64 index = 0u64;
    while (index < input.section_count) {
        ImageSection section = input.section(input.context, index);
        if (section.bytes.length != 0u64) {
            u64 begin = section_base(layout, index);
            result = protect_pages(base, end, begin - end, 0x02);
            if (result.kind != ImageFailure::None) { return result; }
            result = protect_pages(base, begin, section.bytes.length, section_protection(section.flags));
            if (result.kind != ImageFailure::None) { return result; }
            end = aligned(begin + section.bytes.length, PAGE_SIZE);
        }
        index += 1u64;
    }
    if (end < layout.size) {
        result = protect_pages(base, end, layout.size - end, 0x02);
        if (result.kind != ImageFailure::None) { return result; }
    }
    if (FlushInstructionCache(GetCurrentProcess(), base, layout.size) == 0) {
        return failure(ImageFailure::Windows, GetLastError());
    }
    return failure(ImageFailure::None, 0);
}

ImageError add_unwind(inout ImageOwner owner, address table, u32 count) on cpu {
    if (count != 0) {
        if (!reserve_one(owner.registered, 8u64)) { return failure(ImageFailure::Heap, 8); }
        if (RtlAddFunctionTable(table, count, address_bits(owner.base)) == u8(0)) {
            return failure(ImageFailure::Unwind, 0);
        }
        unsafe_position<address>(offset_at(owner.registered.data, owner.registered.count * 8u64)) = table;
        owner.registered.count += 1u64;
    }
    return failure(ImageFailure::None, 0);
}

ImageError register_unwind(ImageInput input, ImageLayout layout, inout ImageOwner owner, address table) on cpu
    requires nonzero(input.section) {
    ImageError result = add_unwind(owner, table, u32(input.code_count));
    if (result.kind != ImageFailure::None) { return result; }
    u64 index = 0u64;
    while (index < input.section_count) {
        ImageSection section = input.section(input.context, index);
        if (section.unwind != 0) {
            result = add_unwind(owner, offset_at(owner.base, section_base(layout, index)),
                u32(section.bytes.length / size_of<RuntimeFunction>()));
            if (result.kind != ImageFailure::None) { return result; }
        }
        index += 1u64;
    }
    return result;
}

extern "C" void release_image(address owner) on cpu {
    ImageOwner image = unsafe_position<ImageOwner>(owner);
    u64 table_index = 0u64;
    while (table_index < image.registered.count) {
        address table = unsafe_position<address>(offset_at(image.registered.data, table_index * 8u64));
        RtlDeleteFunctionTable(table);
        table_index += 1u64;
    }
    if (address_bits(image.base) != 0u64) { VirtualFree(image.base, 0u64, 0x8000); }
    u64 library_index = 0u64;
    while (library_index < image.libraries.count) {
        handle library = unsafe_position<handle>(offset_at(image.libraries.data, library_index * size_of<handle>()));
        release_native_library(library);
        library_index += 1u64;
    }
    free_bytes(image.registered.data);
    free_bytes(image.libraries.data);
    free_bytes(owner);
}

ImageError load_contents(ImageInput input, inout ImageLayout layout, inout ImageOwner owner, address table) on cpu
    requires nonzero(input.code) requires nonzero(input.code_patch)
    requires nonzero(input.section) requires nonzero(input.section_patch)
    requires nonzero(input.symbol) requires nonzero(input.resource) {
    copy_code(input, owner.base, table);
    ImageError result = bind_slots(input, layout, owner);
    if (result.kind != ImageFailure::None) { return result; }
    result = patch_sections(input, layout, owner.base);
    if (result.kind != ImageFailure::None) { return result; }
    result = patch_code(input, layout, owner, table);
    if (result.kind != ImageFailure::None) { return result; }
    result = protect_image(input, layout, owner.base);
    if (result.kind != ImageFailure::None) { return result; }
    return register_unwind(input, layout, owner, table);
}

ImageResult load_planned(ImageInput input, inout ImageLayout layout) on cpu
    requires nonzero(input.code) requires nonzero(input.code_patch)
    requires nonzero(input.section) requires nonzero(input.section_patch)
    requires nonzero(input.symbol) requires nonzero(input.resource) {
    address allocation = HeapAlloc(GetProcessHeap(), 0, size_of<ImageOwner>());
    if (address_bits(allocation) == 0u64) { return empty_result(failure(ImageFailure::Heap, 8)); }
    address base = VirtualAlloc(unsafe_address(0u64), layout.size, 0x3000, 0x04);
    if (address_bits(base) == 0u64) {
        u32 error = GetLastError();
        free_bytes(allocation);
        return empty_result(failure(ImageFailure::Windows, error));
    }
    ImageOwner owner = ImageOwner(base, empty_rows(), empty_rows());
    address table = offset_at(base, layout.table_offset);
    ImageError result = load_contents(input, layout, owner, table);
    unsafe_position<ImageOwner>(allocation) = owner;
    if (result.kind != ImageFailure::None) {
        release_image(allocation);
        return empty_result(result);
    }
    array<u8,20> count_name = "__blink_module_count";
    address count = find_address(input, layout, base, table, ReadBytes(address_of(count_name), 20u64));
    address factories = unsafe_address(0u64);
    if (address_bits(count) != 0u64) {
        array<u8,24> factory_name = "__blink_module_factories";
        factories = find_address(input, layout, base, table, ReadBytes(address_of(factory_name), 24u64));
    }
    return ImageResult(allocation, base, table, count, factories, result);
}

extern "C" ImageResult load_image(ImageInput input) on cpu
    requires nonzero(input.code) requires nonzero(input.code_patch)
    requires nonzero(input.section) requires nonzero(input.section_patch)
    requires nonzero(input.symbol) requires nonzero(input.resource) {
    if (input.code_count == 0u64 && input.has_module == 0) { return empty_result(failure(ImageFailure::None, 0)); }
    LayoutResult plan = plan_image(input);
    ImageResult result = plan.error.kind == ImageFailure::None ? load_planned(input, plan.layout) : empty_result(plan.error);
    free_bytes(plan.layout.sections);
    free_bytes(plan.layout.imports.data);
    free_bytes(plan.layout.keys.data);
    return result;
}

extern "C" MemoryResult query_image_memory(address entry) on cpu {
    MemoryInformation info;
    if (VirtualQuery(entry, info, size_of<MemoryInformation>()) != 0u64) { return MemoryResult(info, 0); }
    u32 error = GetLastError();
    address empty = unsafe_address(0u64);
    return MemoryResult(MemoryInformation(empty, empty, 0, u16(0), 0u64, 0, 0, 0), error);
}
