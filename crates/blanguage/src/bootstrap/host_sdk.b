module bootstrap.host_sdk;

extern "C" i32 RegGetValueW(handle key, address subkey, address value, u32 flags,
    address type_output, address data, inout u32 size) on cpu from "ADVAPI32.dll";
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";

const handle HKEY_LOCAL_MACHINE = unsafe_handle(0xffffffff80000002u64);
const u32 READ_64BIT_STRING = 0x10010002;
const i32 ERROR_NOT_ENOUGH_MEMORY = i32(8);

extern "C" SdkRoot read_sdk_root() on cpu {
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
    if (status != i32(0)) { return SdkRoot(address(), 0, status); }

    // RegGetValueW includes the terminator on success. Reserve its WCHAR
    // even when the stored REG_SZ value omitted it.
    size += 2;
    handle heap = GetProcessHeap();
    address path = HeapAlloc(heap, 0, usize(size));
    if (address_bits(path) == 0) { return SdkRoot(address(), 0, ERROR_NOT_ENOUGH_MEMORY); }

    status = RegGetValueW(HKEY_LOCAL_MACHINE, key_address, name_address, READ_64BIT_STRING,
        address(), path, size);
    if (status != i32(0)) {
        HeapFree(heap, 0, path);
        return SdkRoot(address(), 0, status);
    }
    return SdkRoot(path, size / 2 - 1, i32(0));
}

extern "C" i32 release_sdk_root(address path) on cpu {
    return HeapFree(GetProcessHeap(), 0, path);
}

struct VersionPart {
    u32 value;
    usize next;
    bool valid;
};

u32 version_unit(SdkVersionName name, usize index) on cpu {
    return u32(unsafe_position<u16>(unsafe_address(address_bits(name.data) + index * 2u64)));
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

extern "C" bool select_sdk_version(SdkVersionName name, SdkVersionName selected) on cpu {
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
