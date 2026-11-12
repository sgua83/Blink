module bootstrap.host_sdk;
import blink.windows.directory;
import blink.windows.error;

const handle INVALID_SEARCH = unsafe_handle(0xffffffffffffffffu64);
const u32 ERROR_FILE_NOT_FOUND = 2;
const u32 ERROR_NO_MORE_FILES = 18;
const u32 FILE_ATTRIBUTE_DIRECTORY = 0x10;

extern "C" handle FindFirstFileExW(address pattern, i32 information, write FindData data,
    i32 search, address filter, u32 flags) on cpu
    ensures initialized(data, returned != INVALID_SEARCH) from "KERNEL32.dll";

extern "C" i32 RegGetValueW(handle key, address subkey, address value, u32 flags,
    address type_output, address data, inout u32 size) on cpu from "ADVAPI32.dll";
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" i32 MultiByteToWideChar(u32 page, u32 flags, address input, i32 bytes,
    address output, i32 capacity) on cpu from "KERNEL32.dll";

const handle HKEY_LOCAL_MACHINE = unsafe_handle(0xffffffff80000002u64);
const u32 READ_64BIT_STRING = 0x10010002;
const i32 ERROR_NOT_ENOUGH_MEMORY = i32(8);

extern "C" HostWide read_sdk_root() on cpu {
    let subkey = array<u16,48>(
        u16(83), u16(79), u16(70), u16(84), u16(87), u16(65), u16(82), u16(69), u16(92), u16(77), u16(105), u16(99),
        u16(114), u16(111), u16(115), u16(111), u16(102), u16(116), u16(92), u16(87), u16(105), u16(110), u16(100), u16(111),
        u16(119), u16(115), u16(32), u16(75), u16(105), u16(116), u16(115), u16(92), u16(73), u16(110), u16(115), u16(116),
        u16(97), u16(108), u16(108), u16(101), u16(100), u16(32), u16(82), u16(111), u16(111), u16(116), u16(115), u16(0)
    );
    let name = array<u16,11>(
        u16(75), u16(105), u16(116), u16(115), u16(82), u16(111), u16(111), u16(116), u16(49), u16(48), u16(0)
    );
    address key_address = address_of(subkey[0]);
    address name_address = address_of(name[0]);
    u32 size = 0;
    i32 status = RegGetValueW(HKEY_LOCAL_MACHINE, key_address, name_address, READ_64BIT_STRING,
        address(), address(), size);
    if (status != i32(0)) { return HostWide(address(), 0, 0, status); }

    // RegGetValueW includes the terminator on success. Reserve its WCHAR
    // even when the stored REG_SZ value omitted it.
    size += 2;
    handle heap = GetProcessHeap();
    address path = HeapAlloc(heap, 0, usize(size));
    if (address_bits(path) == 0) { return HostWide(address(), 0, 0, ERROR_NOT_ENOUGH_MEMORY); }

    status = RegGetValueW(HKEY_LOCAL_MACHINE, key_address, name_address, READ_64BIT_STRING,
        address(), path, size);
    if (status != i32(0)) {
        HeapFree(heap, 0, path);
        return HostWide(address(), 0, 0, status);
    }
    return HostWide(path, 0, size / 2 - 1, i32(0));
}

extern "C" i32 release_host_wide(address data) on cpu {
    return HeapFree(GetProcessHeap(), 0, data);
}

u32 wide_unit(address data, usize index) on cpu {
    return u32(unsafe_position<u16>(unsafe_address(address_bits(data) + index * 2u64)));
}

bool version_space(u32 unit) on cpu {
    return (unit >= 0x9 && unit <= 0xd) || unit == 0x20 || unit == 0x85 || unit == 0xa0
        || unit == 0x1680 || (unit >= 0x2000 && unit <= 0x200a) || unit == 0x2028
        || unit == 0x2029 || unit == 0x202f || unit == 0x205f || unit == 0x3000;
}

extern "C" HostWide decode_host_text(address input, i32 bytes, HostText kind) on cpu {
    u32 page = kind == HostText::LinkOutput ? 0 : 65001;
    i32 capacity = MultiByteToWideChar(page, 8, input, bytes, address(), i32(0));
    if (capacity == i32(0)) { return HostWide(address(), 0, 0, i32(GetLastError())); }

    handle heap = GetProcessHeap();
    address data = HeapAlloc(heap, 0, usize(capacity) * 2u64);
    if (address_bits(data) == 0) { return HostWide(address(), 0, 0, ERROR_NOT_ENOUGH_MEMORY); }
    i32 count = MultiByteToWideChar(page, 8, input, bytes, data, capacity);
    if (count == i32(0)) {
        i32 status = i32(GetLastError());
        HeapFree(heap, 0, data);
        return HostWide(address(), 0, 0, status);
    }

    u32 first = 0;
    u32 end = u32(count);
    if (kind == HostText::ToolVersion) {
        while (first < end && version_space(wide_unit(data, usize(first)))) { first += 1; }
        while (first < end && version_space(wide_unit(data, usize(end - 1)))) { end -= 1; }
    } else if (kind == HostText::InstallationPath) {
        while (end > 0) {
            u32 unit = wide_unit(data, usize(end - 1));
            if (unit != 13 && unit != 10) { break; }
            end -= 1;
        }
    }
    return HostWide(data, first, end - first, i32(0));
}

struct SdkVersionName {
    address data;
    usize units;
};

struct VersionPart {
    u32 value;
    usize next;
    bool valid;
};

u32 version_unit(SdkVersionName name, usize index) on cpu {
    return wide_unit(name.data, index);
}

VersionPart version_part(SdkVersionName name, usize start) on cpu {
    usize index = start;
    if (index < name.units && version_unit(name, index) == 43) { index += 1u64; }
    usize first_digit = index;
    u32 number = 0;
    while (index < name.units) {
        u32 unit = version_unit(name, index);
        if (unit == 46) { break; }
        if (unit < 48 || unit > 57) { return VersionPart(0, index, false); }
        u32 digit = unit - 48;
        if (number > (0xffffffff - digit) / 10) { return VersionPart(0, index, false); }
        number = number * 10 + digit;
        index += 1u64;
    }
    return VersionPart(number, index + 1u64, index != first_digit);
}

bool select_sdk_version(SdkVersionName name, SdkVersionName selected) on cpu {
    usize index = 0u64;
    usize selected_index = 0u64;
    bool equal_prefix = selected.units != 0u64;
    while (index <= name.units) {
        VersionPart part = version_part(name, index);
        if (!part.valid) { return false; }
        if (equal_prefix && selected_index <= selected.units) {
            VersionPart previous = version_part(selected, selected_index);
            if (part.value < previous.value) { return false; }
            equal_prefix = part.value == previous.value;
            selected_index = previous.next;
        } else {
            equal_prefix = false;
        }
        index = part.next;
    }
    // For equal versions, the current directory entry replaces the previous one.
    return !equal_prefix || selected_index > selected.units;
}

extern "C" SdkVersionResult find_sdk_version(address pattern, address selected) on cpu {
    FindData data;
    // FindExInfoBasic omits the unused alternate filename.
    handle search = FindFirstFileExW(pattern, i32(1), data, i32(0), address(), 0);
    if (search == INVALID_SEARCH) {
        u32 status = GetLastError();
        if (status == ERROR_FILE_NOT_FOUND) { return SdkVersionResult(0, 0); }
        return SdkVersionResult(0, status);
    }

    usize selected_units = 0u64;
    i32 more = i32(1);
    while (more != i32(0)) {
        if ((data.attributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
            address name = address_of(data.name[0]);
            SdkVersionName entry = SdkVersionName(name, 0u64);
            while (version_unit(entry, entry.units) != 0) { entry.units += 1u64; }
            if (select_sdk_version(entry, SdkVersionName(selected, selected_units))) {
                usize index = 0u64;
                while (index < entry.units) {
                    address unit = unsafe_address(address_bits(selected) + index * 2u64);
                    unsafe_position<u16>(unit) = u16(version_unit(entry, index));
                    index += 1u64;
                }
                selected_units = entry.units;
            }
        }
        more = FindNextFileW(search, data);
    }
    u32 status = GetLastError();
    if (FindClose(search) == i32(0) && status == ERROR_NO_MORE_FILES) {
        return SdkVersionResult(0, GetLastError());
    }
    if (status != ERROR_NO_MORE_FILES) { return SdkVersionResult(0, status); }
    return SdkVersionResult(u32(selected_units), 0);
}
