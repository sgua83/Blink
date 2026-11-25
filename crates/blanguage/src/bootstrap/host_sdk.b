module bootstrap.host_sdk;
import blink.windows.directory;
import blink.windows.error;
import blink.windows.file;
import blink.windows.system;
import blink.windows.path_core;
import blink.windows.process;
import blink.windows.process_extended;
import blink.windows.thread;
import bootstrap.metadata;

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

extern "C" i32 WideCharToMultiByte(u32 page, u32 flags, address input, i32 units,
    address output, i32 capacity, address default_character, address used_default) on cpu from "KERNEL32.dll";

const u32 UTF8 = 65001;
const u32 REJECT_INVALID_UTF16 = 128;

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

extern "C" SdkVersionResult find_sdk_version(address pattern, usize units, address selected) on cpu {
    NativePath path = native_path(pattern, units, true);
    if (path.status != 0) { return SdkVersionResult(0, path.status); }
    FindData data;
    // FindExInfoBasic omits the unused alternate filename.
    handle search = FindFirstFileExW(path.name, i32(1), data, i32(0), address(), 0);
    u32 open_status = search == INVALID_SEARCH ? GetLastError() : 0;
    release_native_path(path);
    if (search == INVALID_SEARCH) {
        if (open_status == ERROR_FILE_NOT_FOUND) { return SdkVersionResult(0, 0); }
        return SdkVersionResult(0, open_status);
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

const u32 FILE_FLAG_OPEN_REPARSE_POINT = 0x00200000;
const u32 GENERIC_READ_WRITE = 0xc0000000;
const u32 GENERIC_WRITE = 0x40000000;
const u32 CREATE_NEW = 1;
const u32 DELETE_ACCESS = 65536;
const u32 OPEN_EXISTING = 3;
const u32 OPEN_ALWAYS = 4;
const u32 ERROR_ALREADY_EXISTS = 183;
const i32 FILE_ALLOCATION_INFO = i32(5);

const u32 SHARE_READ = 1;
const u32 SHARE_ALL = 7;
const u32 FILE_ATTRIBUTE_NORMAL = 128;

HostFileOpen open_host_file(address path, usize units, u32 access, u32 share, u32 creation, u32 flags) on cpu {
    NativePath name = native_path(path, units, true);
    if (name.status != 0) { return HostFileOpen(INVALID_HANDLE_VALUE, name.status); }

    handle file = CreateFileW(name.name, access, share, address(), creation, flags, handle());
    u32 status = file == INVALID_HANDLE_VALUE || creation == OPEN_ALWAYS ? GetLastError() : 0;
    release_native_path(name);
    return HostFileOpen(file, status);
}

extern "C" HostFileOpen open_shared_read(address path, usize units) on cpu {
    return open_host_file(path, units, GENERIC_READ, SHARE_READ, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL);
}

HostWriteResult write_host_bytes(handle file, address data, usize bytes) on cpu {
    HostWriteResult result = HostWriteResult(HostWriteKind::Written, 0);
    usize completed = 0u64;
    while (completed < bytes) {
        usize remaining = bytes - completed;
        u32 request = remaining > 0xffffffffu64 ? 0xffffffff : u32(remaining);
        address buffer = unsafe_address(address_bits(data) + completed);
        u32 written = 0;
        if (WriteFile(file, buffer, request, written, address()) == i32(0)) {
            result = HostWriteResult(HostWriteKind::WriteError, GetLastError());
            break;
        }
        if (written == 0) {
            result = HostWriteResult(HostWriteKind::NoProgress, 0);
            break;
        }
        completed += usize(written);
    }
    return result;
}

extern "C" HostWriteResult write_host_input(address path, usize units, address data, usize bytes) on cpu {
    HostFileOpen opened = open_host_file(path, units, GENERIC_READ_WRITE, SHARE_ALL, CREATE_NEW, FILE_FLAG_OPEN_REPARSE_POINT);
    if (opened.file == INVALID_HANDLE_VALUE) {
        return HostWriteResult(HostWriteKind::OpenError, opened.status);
    }
    HostWriteResult written = write_host_bytes(opened.file, data, bytes);
    CloseHandle(opened.file);
    return written;
}

extern "C" HostWriteResult write_host_output(address path, usize units, address data, usize bytes) on cpu {
    HostFileOpen opened = open_host_file(path, units, GENERIC_WRITE, SHARE_ALL, OPEN_ALWAYS, 0);
    if (opened.file == INVALID_HANDLE_VALUE) {
        return HostWriteResult(HostWriteKind::OpenError, opened.status);
    }

    if (opened.status == ERROR_ALREADY_EXISTS) {
        // Truncate the existing file through its opened handle, preserving its identity.
        i64 length = i64(0);
        if (SetFileInformationByHandle(opened.file, FILE_ALLOCATION_INFO, address_of(length),
                u32(size_of<i64>())) == i32(0)) {
            u32 status = GetLastError();
            CloseHandle(opened.file);
            return HostWriteResult(HostWriteKind::WriteError, status);
        }
    }
    HostWriteResult written = write_host_bytes(opened.file, data, bytes);
    CloseHandle(opened.file);
    return written;
}

LinkOutputResult link_output_result(LinkOutputFailure failure) on cpu {
    return LinkOutputResult(HostWriteResult(HostWriteKind::Written, 0), failure);
}

LinkOutputResult link_output_io(u32 status) on cpu {
    return LinkOutputResult(HostWriteResult(HostWriteKind::OpenError, status), LinkOutputFailure::None);
}

HostWriteResult write_link_companion(UnitSpan target, UnitSpan extension, MetadataBytes content) on cpu {
    ExtensionPlan plan = extension_prepare(target, extension.length);
    handle heap = GetProcessHeap();
    address path = HeapAlloc(heap, 0, (plan.length + 1u64) * 2u64);
    if (address_bits(path) == 0u64) {
        return HostWriteResult(HostWriteKind::OpenError, u32(ERROR_NOT_ENOUGH_MEMORY));
    }
    extension_encode(target, extension, plan, path);
    unsafe_position<u16>(unsafe_address(address_bits(path) + plan.length * 2u64)) = u16(0);
    HostWriteResult written = write_host_output(path, plan.length, content.bytes, content.length);
    HeapFree(heap, 0, path);
    return written;
}

HostWriteResult write_link_bindings(UnitSpan path, LinkBindings bindings, MetadataBytes library) on cpu {
    let header = array<u16,5>(u16(97), u16(98), u16(105), u16(46), u16(104));
    HostWriteResult written = write_link_companion(path, UnitSpan(address_of(header[0]), 5u64), bindings.header);
    if (written.kind != HostWriteKind::Written) { return written; }

    MetadataBytes binding = bindings.rust;
    usize size = rust_link_binding(binding, bindings.prefix, library, address());
    address rendered = address();
    handle heap = GetProcessHeap();
    if (size != 0u64) {
        rendered = HeapAlloc(heap, 0, size);
        if (address_bits(rendered) == 0u64) {
            return HostWriteResult(HostWriteKind::OpenError, u32(ERROR_NOT_ENOUGH_MEMORY));
        }
        usize bytes = rust_link_binding(binding, bindings.prefix, library, rendered);
        binding = MetadataBytes(rendered, bytes);
    }
    let rust = array<u16,2>(u16(114), u16(115));
    written = write_link_companion(path, UnitSpan(address_of(rust[0]), 2u64), binding);
    if (address_bits(rendered) != 0u64) { HeapFree(heap, 0, rendered); }
    return written;
}

LinkOutputResult link_dll(UnitSpan path, LinkBindings bindings, address context,
    cfn<u32,address,u64,u32> msvc) on cpu requires nonzero(msvc) {
    UnitSpan stem = file_stem(path);
    if (stem.length == 0u64) { return link_output_result(LinkOutputFailure::LibraryName); }
    i32 size = WideCharToMultiByte(UTF8, REJECT_INVALID_UTF16, stem.data, i32(stem.length),
        address(), i32(0), address(), address());
    if (size == i32(0)) { return link_output_result(LinkOutputFailure::LibraryName); }
    handle heap = GetProcessHeap();
    address bytes = HeapAlloc(heap, 0, usize(size));
    if (address_bits(bytes) == 0u64) { return link_output_io(u32(ERROR_NOT_ENOUGH_MEMORY)); }
    i32 written = WideCharToMultiByte(UTF8, REJECT_INVALID_UTF16, stem.data, i32(stem.length),
        bytes, size, address(), address());
    LinkOutputResult result = link_output_result(LinkOutputFailure::None);
    if (written == i32(0)) {
        result.failure = LinkOutputFailure::LibraryName;
    } else if (msvc(context, 0u64, 1) != 0) {
        result.written = write_link_bindings(path, bindings, MetadataBytes(bytes, usize(written)));
    } else {
        result.failure = LinkOutputFailure::Native;
    }
    HeapFree(heap, 0, bytes);
    return result;
}

extern "C" LinkOutputResult deliver_link_output(UnitSpan path, UnitSpan parent, EntryPlan plan, LinkBindings bindings, address context,
    cfn<HostFilePart,address,u64> pe, cfn<u32,address,u64,u32> msvc) on cpu
    requires nonzero(pe) requires nonzero(msvc) {
    u32 directory = create_output_directories(parent.data, parent.length);
    if (directory != 0) { return link_output_io(directory); }
    if (plan.product == EntryProduct::Pe) {
        HostFilePart image = pe(context, plan.stack_reserve);
        if (address_bits(image.data) == 0u64) { return link_output_result(LinkOutputFailure::Native); }
        return LinkOutputResult(write_host_output(path.data, path.length, image.data, image.bytes),
            LinkOutputFailure::None);
    }
    if (plan.product == EntryProduct::HostApplication) {
        return link_output_result(msvc(context, plan.stack_reserve, 0) != 0
            ? LinkOutputFailure::None : LinkOutputFailure::Native);
    }
    if (plan.product == EntryProduct::Dll) { return link_dll(path, bindings, context, msvc); }
    return link_output_result(plan.product == EntryProduct::DllWithoutArchive
        ? LinkOutputFailure::MissingArchive : LinkOutputFailure::NoProduct);
}

extern "C" u32 remove_host_input(address path, usize units) on cpu {
    HostFileOpen opened = open_host_file(path, units, DELETE_ACCESS, SHARE_ALL, OPEN_EXISTING, FILE_FLAG_OPEN_REPARSE_POINT);
    if (opened.file == INVALID_HANDLE_VALUE) { return opened.status; }

    // FILE_DISPOSITION_INFO_EX: delete the file, using POSIX semantics and ignoring its read-only attribute.
    u32 disposition = 19;
    i32 removed = SetFileInformationByHandle(opened.file, i32(21), address_of(disposition), u32(size_of<u32>()));
    u32 status = removed == i32(0) ? GetLastError() : 0;
    CloseHandle(opened.file);
    return status;
}


const u32 ERROR_ACCESS_DENIED = 5;
const u32 ERROR_SHARING_VIOLATION = 32;

struct HostReplacement { u32 status; bool moved; };

HostReplacement replace_host_file(address source,usize source_units,
    address target,usize target_units) on cpu {
    NativePath from = native_path(source,source_units,true);
    if (from.status!=0) { return HostReplacement(from.status,false); }
    NativePath to = native_path(target,target_units,true);
    if (to.status!=0) {
        release_native_path(from);
        return HostReplacement(to.status,false);
    }
    result<void,RenameError> outcome = rename_path(from.name,UnitSpan(to.name,to.units));
    release_native_path(to);
    release_native_path(from);
    match (outcome) {
        Ok() { return HostReplacement(0,true); }
        Err(error) {
            match (error) {
                LengthOverflow() { return HostReplacement(206,false); }
                Nul(index) { return HostReplacement(123,false); }
                Change(change) {
                    u32 status = 0;
                    match (change) {
                        Open(code) { return HostReplacement(code,false); }
                        Apply(code) { status=code; }
                        Close(cause) { return HostReplacement(cause.code,true); }
                        ApplyAndClose(cause) { status=cause.code; }
                    }
                    // The build interface reports a denied destination replacement as access denied.
                    return HostReplacement(status==ERROR_SHARING_VIOLATION ? ERROR_ACCESS_DENIED : status,false);
                }
            }
        }
    }
}

extern "C" HostPublishResult publish_host_file(address target,usize target_units,
    address temporary,usize temporary_units,address parts,usize count) on cpu {
    HostFileOpen opened = open_host_file(temporary, temporary_units, GENERIC_READ_WRITE, SHARE_ALL, CREATE_NEW, FILE_FLAG_OPEN_REPARSE_POINT);
    if (opened.file==INVALID_HANDLE_VALUE) {
        return HostPublishResult(HostWriteResult(HostWriteKind::OpenError,opened.status),0,0);
    }
    HostWriteResult written = HostWriteResult(HostWriteKind::Written,0);
    usize index = 0u64;
    while (index<count) {
        HostFilePart part = unsafe_position<HostFilePart>(unsafe_address(address_bits(parts)+index*size_of<HostFilePart>()));
        written = write_host_bytes(opened.file,part.data,part.bytes);
        if (written.kind!=HostWriteKind::Written) { break; }
        index+=1u64;
    }
    CloseHandle(opened.file);

    u32 replacement = 0;
    if (written.kind==HostWriteKind::Written) {
        HostReplacement result = replace_host_file(temporary,temporary_units,target,target_units);
        replacement=result.status;
        if (result.moved) { return HostPublishResult(written,replacement,0); }
    }
    return HostPublishResult(written,replacement,remove_host_input(temporary,temporary_units));
}

extern "C" i32 CreatePipe(inout handle read_end, inout handle write_end,
    address attributes, u32 bytes) on cpu
    from "KERNEL32.dll";

const u32 DETACHED_PROCESS = 0x00000008;
const u32 GENERIC_READ = 0x80000000;

struct HostPipe { handle read_end; handle write_end; u32 status; };

HostPipe host_pipe() on cpu {
    handle read_end = handle();
    handle write_end = handle();
    if (CreatePipe(read_end, write_end, address(), 0) == i32(0)) {
        return HostPipe(handle(), handle(), GetLastError());
    }
    if (SetHandleInformation(write_end, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == i32(0)) {
        u32 status = GetLastError();
        CloseHandle(read_end);
        CloseHandle(write_end);
        return HostPipe(handle(), handle(), status);
    }
    return HostPipe(read_end, write_end, 0);
}

struct HostChild { handle process; u32 status; };

HostChild create_host_child(address executable,address command_line,address directory,u32 flags,
    read array<handle,3> stdio,handle job) on cpu {
    usize size = 0u64;
    u32 count=job==handle() ? 1 : 2;
    InitializeProcThreadAttributeList(address(), count, 0, size);
    if (size == 0u64) { return HostChild(handle(), GetLastError()); }

    handle heap = GetProcessHeap();
    address attributes = HeapAlloc(heap, 0, size);
    if (address_bits(attributes) == 0u64) {
        return HostChild(handle(), u32(ERROR_NOT_ENOUGH_MEMORY));
    }
    if (InitializeProcThreadAttributeList(attributes, count, 0, size) == i32(0)) {
        u32 status = GetLastError();
        HeapFree(heap, 0, attributes);
        return HostChild(handle(), status);
    }

    HostChild child = HostChild(handle(), 0);
    if (UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            address_of(stdio[0]), size_of<array<handle,3>>(), address(), address()) == i32(0)) {
        child.status = GetLastError();
    }
    if(child.status==0 && job!=handle()){
        if(UpdateProcThreadAttribute(attributes,0,PROC_THREAD_ATTRIBUTE_JOB_LIST,address_of(job),
                size_of<handle>(),address(),address())==i32(0)){child.status=GetLastError();}
    }
    if(child.status==0){
        StartupInfo startup = StartupInfo(u32(size_of<StartupInfoEx>()), address(), address(), address(),
            0, 0, 0, 0, 0, 0, 0, STARTF_USESTDHANDLES, u16(0), u16(0), address(),
            stdio[0], stdio[1], stdio[2]);
        StartupInfoEx extended = StartupInfoEx(startup, attributes);
        ProcessInformation info;
        if (blink.windows.process_extended::CreateProcessW(executable, command_line, address(), address(),
                i32(1), EXTENDED_STARTUPINFO_PRESENT | flags, address(), directory,
                extended, info) == i32(0)) {
            child.status = GetLastError();
        } else {
            child.process = info.process;
            CloseHandle(info.thread);
        }
    }
    DeleteProcThreadAttributeList(attributes);
    HeapFree(heap, 0, attributes);
    return child;
}


struct HostCommandLine { address data; usize units; };

void append_host_unit(inout HostCommandLine line, u32 unit) on cpu {
    address destination = unsafe_address(address_bits(line.data) + line.units * 2u64);
    unsafe_position<u16>(destination) = u16(unit);
    line.units += 1u64;
}

void append_host_slashes(inout HostCommandLine line, usize count) on cpu {
    usize remaining = count;
    while (remaining != 0u64) {
        append_host_unit(line, 92);
        remaining -= 1u64;
    }
}

void append_host_argument(inout HostCommandLine line, address arguments, usize first,
    usize end, bool quoted) on cpu {
    append_host_unit(line, 32);
    if (quoted) { append_host_unit(line, 34); }

    usize backslashes = 0u64;
    usize index = first;
    while (index < end) {
        u32 unit = wide_unit(arguments, index);
        if (unit == 34) {
            // The child parses an odd backslash count before a literal quote.
            append_host_slashes(line, backslashes + 1u64);
        }
        append_host_unit(line, unit);
        backslashes = unit == 92 ? backslashes + 1u64 : 0u64;
        index += 1u64;
    }
    if (quoted) {
        append_host_slashes(line, backslashes);
        append_host_unit(line, 34);
    }
}

address host_command_line(address executable, usize units, address arguments, usize argument_units) on cpu {
    // Each argument may double its units and use three units for space and quotes.
    // The input already reserves one terminator per argument.
    usize capacity = units + 3u64 + argument_units * 3u64;
    address data = HeapAlloc(GetProcessHeap(), 0, capacity * 2u64);
    if (address_bits(data) == 0u64) { return data; }

    HostCommandLine line = HostCommandLine(data, 0u64);
    append_host_unit(line, 34);
    usize index = 0u64;
    while (index < units) {
        append_host_unit(line, wide_unit(executable, index));
        index += 1u64;
    }
    append_host_unit(line, 34);

    usize first = 0u64;
    while (first < argument_units) {
        usize end = first;
        bool quoted = false;
        u32 unit = wide_unit(arguments, end);
        while (unit != 0) {
            quoted = quoted || unit == 32 || unit == 9;
            end += 1u64;
            unit = wide_unit(arguments, end);
        }
        append_host_argument(line, arguments, first, end, quoted || end == first);
        first = end + 1u64;
    }
    append_host_unit(line, 0);
    return data;
}

HostChild start_host_child(address executable, usize units, address arguments,
    usize argument_units, handle stdout, handle stderr) on cpu {
    NativePath path = native_path(executable, units, true);
    if (path.status != 0) { return HostChild(handle(), path.status); }

    let name = array<u16,4>(u16(78), u16(85), u16(76), u16(0)); // NUL
    handle stdin = CreateFileW(address_of(name[0]), GENERIC_READ, 7, address(), OPEN_EXISTING, 0, handle());
    HostChild child = HostChild(handle(), 0);
    if (stdin == INVALID_HANDLE_VALUE) {
        child.status = GetLastError();
    } else {
        if (SetHandleInformation(stdin, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == i32(0)) {
            child.status = GetLastError();
        } else {
            let stdio = array<handle,3>(stdin, stdout, stderr);
            address command_line = host_command_line(executable, units, arguments, argument_units);
            if (address_bits(command_line) == 0u64) {
                child.status = u32(ERROR_NOT_ENOUGH_MEMORY);
            } else {
                child = create_host_child(path.name,command_line,address(),DETACHED_PROCESS,stdio,handle());
                release_host_wide(command_line);
            }
        }
        CloseHandle(stdin);
    }
    release_native_path(path);
    return child;
}

struct HostReadWork { handle pipe; SourceBytes bytes; };

extern "C" u32 read_host_stderr(address parameter) on cpu {
    handle pipe = unsafe_position<HostReadWork>(parameter).pipe;
    SourceBytes bytes = read_source(pipe);
    CloseHandle(pipe);
    unsafe_position<HostReadWork>(parameter).bytes = bytes;
    return 0;
}

extern "C" HostProcessOutput run_host_process(address executable, usize units,
    address arguments, usize argument_units) on cpu {
    SourceBytes empty = SourceBytes(address(), 0u64, StorageError(0, 0));
    HostPipe stdout = host_pipe();
    if (stdout.status != 0) { return HostProcessOutput(empty, empty, 0, stdout.status); }
    HostPipe stderr = host_pipe();
    if (stderr.status != 0) {
        CloseHandle(stdout.read_end);
        CloseHandle(stdout.write_end);
        return HostProcessOutput(empty, empty, 0, stderr.status);
    }

    HostChild child = start_host_child(executable, units, arguments, argument_units,
        stdout.write_end, stderr.write_end);
    CloseHandle(stdout.write_end);
    CloseHandle(stderr.write_end);
    if (child.status != 0) {
        CloseHandle(stdout.read_end);
        CloseHandle(stderr.read_end);
        return HostProcessOutput(empty, empty, 0, child.status);
    }

    // Each blocking reader owns its pipe end. Join before the worker's context leaves scope.
    HostReadWork work = HostReadWork(stderr.read_end, empty);
    handle reader = CreateThread(address(), 0u64, c_function(read_host_stderr), address_of(work), 0, address());
    if (reader == handle()) {
        u32 status = GetLastError();
        CloseHandle(stdout.read_end);
        CloseHandle(stderr.read_end);
        WaitForSingleObject(child.process, INFINITE);
        CloseHandle(child.process);
        return HostProcessOutput(empty, empty, 0, status);
    }
    SourceBytes output = read_source(stdout.read_end);
    CloseHandle(stdout.read_end);
    WaitForSingleObject(reader, INFINITE);
    CloseHandle(reader);

    HostProcessExit result=finish_host_child(child.process);
    return HostProcessOutput(output,work.bytes,result.exit_code,result.status);
}

HostProcessExit finish_host_child(handle process) on cpu {
    u32 exit_code=0;u32 status=0;
    if(WaitForSingleObject(process,INFINITE)!=0){status=GetLastError();}
    else {u32 code;if(GetExitCodeProcess(process,code)!=i32(0)){exit_code=code;}else{status=GetLastError();}}
    CloseHandle(process);return HostProcessExit(exit_code,status);
}

extern "C" HostProcessExit run_compiler_process(address directory,address arguments,
    usize argument_units,address stdio,usize mask,u16 group) on cpu {
    u32 i=0;while(i<3){
        handle item=unsafe_position<handle>(unsafe_address(address_bits(stdio)+usize(i)*size_of<handle>()));
        if(SetHandleInformation(item,HANDLE_FLAG_INHERIT,HANDLE_FLAG_INHERIT)==i32(0)){
            return HostProcessExit(0,GetLastError());
        }i+=1;
    }
    handle job=handle();
    if(mask!=0u64){
        job=CreateJobObjectW(address(),address());
        if(job==handle()){return HostProcessExit(0,GetLastError());}
        GroupAffinity affinity=GroupAffinity(mask,group,array<u16,3>(u16(0),u16(0),u16(0)));
        if(SetInformationJobObject(job,i32(14),address_of(affinity),u32(size_of<GroupAffinity>()))==i32(0)){
            u32 status=GetLastError();CloseHandle(job);return HostProcessExit(0,status);
        }
    }
    let cargo=array<u16,10>(u16(99),u16(97),u16(114),u16(103),u16(111),u16(46),u16(101),u16(120),u16(101),u16(0));
    address command=host_command_line(address_of(cargo[0]),9u64,arguments,argument_units);
    HostProcessExit result=HostProcessExit(0,u32(ERROR_NOT_ENOUGH_MEMORY));
    if(address_bits(command)!=0u64){
        HostChild child=create_host_child(address(),command,directory,CREATE_NO_WINDOW,
            unsafe_position<array<handle,3>>(stdio),job);
        release_host_wide(command);
        result=child.status!=0 ? HostProcessExit(0,child.status) : finish_host_child(child.process);
    }
    if(job!=handle()){CloseHandle(job);}return result;
}

UnitSpan empty_tool_path() on cpu { return UnitSpan(address(),0u64); }

void release_tool_path(UnitSpan path) on cpu {
    if(address_bits(path.data)!=0u64){release_host_wide(path.data);}
}

UnitSpan join_tool_path(UnitSpan base,UnitSpan tail) on cpu {
    handle heap=GetProcessHeap();
    usize rows=join_scratch(base,tail);
    address scratch=address();
    if(rows!=0u64){
        scratch=HeapAlloc(heap,0,rows*size_of<JoinComponent>());
        if(address_bits(scratch)==0u64){return empty_tool_path();}
    }
    JoinPlan plan=join_prepare(base,tail,scratch);
    address output=HeapAlloc(heap,0,(plan.length+1u64)*2u64);
    if(address_bits(output)!=0u64){
        join_encode(base,tail,scratch,plan,output);
        unsafe_position<u16>(unsafe_address(address_bits(output)+plan.length*2u64))=u16(0);
    }
    if(rows!=0u64){HeapFree(heap,0,scratch);}
    return UnitSpan(output,plan.length);
}

extern "C" MsvcLinkResult link_msvc_inputs(MsvcLinkRequest request,
    cfn<MsvcLinkInput,address,MsvcInputKind,usize> input,
    cfn<UnitSpan,address,address,usize> arguments) on cpu
    requires nonzero(input) requires nonzero(arguments) {
    SourceBytes empty = SourceBytes(address(), 0u64, StorageError(0, 0));
    MsvcLinkResult result = MsvcLinkResult(HostWriteResult(HostWriteKind::Written, 0),
        HostProcessOutput(empty, empty, 0, 0), 0);
    usize code_end = request.archives + request.objects;
    usize count = code_end + 2u64;
    usize index = 0u64;
    usize created = 0u64;
    while (index < count) {
        MsvcInputKind kind = MsvcInputKind::Archive;
        usize item = index;
        if (index >= code_end) {
            kind = index == code_end ? MsvcInputKind::Imports : MsvcInputKind::Resources;
            item = 0u64;
        } else if (index >= request.archives) {
            kind = MsvcInputKind::Code;
            item -= request.archives;
        }
        MsvcLinkInput next = input(request.context, kind, item);
        if (next.failed != 0) { break; }
        index += 1u64;
        if (next.filename.length == 0u64) { continue; }

        UnitSpan path = join_tool_path(request.directory, next.filename);
        if (address_bits(path.data) == 0u64) {
            result.written = HostWriteResult(HostWriteKind::OpenError, u32(ERROR_NOT_ENOUGH_MEMORY));
            break;
        }
        result.written = write_host_input(path.data, path.length, next.bytes.data, next.bytes.bytes);
        if (result.written.kind == HostWriteKind::OpenError) {
            release_tool_path(path);
            break;
        }
        unsafe_position<UnitSpan>(unsafe_address(address_bits(request.paths) + created * size_of<UnitSpan>())) = path;
        created += 1u64;
        if (result.written.kind != HostWriteKind::Written) { break; }
    }

    bool linked = false;
    if (index == count && result.written.kind == HostWriteKind::Written) {
        UnitSpan packed = arguments(request.context, request.paths, created);
        if (address_bits(packed.data) != 0u64) {
            result.process = run_host_process(request.executable.data, request.executable.length, packed.data, packed.length);
            linked = result.process.status == 0 && result.process.exit_code == 0 &&
                result.process.stdout.error.kind == 0 && result.process.stderr.error.kind == 0;
        }
    }
    usize at = 0u64;
    while (at < created) {
        UnitSpan path = unsafe_position<UnitSpan>(unsafe_address(address_bits(request.paths) + at * size_of<UnitSpan>()));
        u32 removed = remove_host_input(path.data, path.length);
        release_tool_path(path);
        if (linked && result.cleanup == 0) { result.cleanup = removed; }
        at += 1u64;
    }
    return result;
}

UnitSpan join_tool_literal<const N:u32>(UnitSpan base,read array<u8,N> tail) on cpu {
    array<u16,N> units=array<u16,N>(u16(0));
    for(index in 0..N){units[index]=u16(tail[index]);}
    return join_tool_path(base,UnitSpan(address_of(units[0]),u64(N)));
}

UnitSpan decoded_units(HostWide text) on cpu {
    return UnitSpan(unsafe_address(address_bits(text.data)+u64(text.first)*2u64),u64(text.units));
}

void release_tool_bytes(SourceBytes bytes) on cpu {
    if(address_bits(bytes.data)!=0u64){release_source(bytes.data);}
}

bool tool_path_has_nul(UnitSpan path) on cpu {
    usize index=0u64;
    while(index<path.length){
        if(wide_unit(path.data,index)==0){return true;}
        index+=1u64;
    }
    return false;
}

HostProcessOutput run_vswhere<const N:u32>(UnitSpan installer,read array<u8,N> arguments) on cpu {
    array<u16,N> units=array<u16,N>(u16(0));
    for(index in 0..N){
        u8 byte=arguments[index];
        units[index]=byte==u8(124) ? u16(0) : u16(byte);
    }
    return run_host_process(installer.data,installer.length,address_of(units[0]),u64(N));
}

void fail_tool_allocation(inout MsvcTools tools) on cpu {
    tools.failure=MsvcDiscovery::System;
    tools.status=u32(ERROR_NOT_ENOUGH_MEMORY);
}

void locate_vc_tools(UnitSpan installation,inout MsvcTools tools) on cpu {
    UnitSpan file=join_tool_literal(installation,"VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt");
    if(address_bits(file.data)==0u64){fail_tool_allocation(tools);return;}
    if(tool_path_has_nul(file)){
        release_tool_path(file);tools.failure=MsvcDiscovery::InvalidPath;return;
    }
    HostFileOpen opened=open_host_file(file.data,file.length,GENERIC_READ,SHARE_ALL,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL);
    release_tool_path(file);
    if(opened.status!=0){tools.failure=MsvcDiscovery::System;tools.status=opened.status;return;}
    SourceBytes bytes=read_source(opened.file);
    CloseHandle(opened.file);
    if(bytes.error.kind!=0){
        tools.output.stdout=bytes;tools.failure=MsvcDiscovery::FileRead;return;
    }
    if(bytes.length>2147483647u64){
        release_tool_bytes(bytes);tools.failure=MsvcDiscovery::TextLength;return;
    }
    HostWide version=HostWide(address(),0,0,i32(0));
    if(bytes.length!=0u64){version=decode_host_text(bytes.data,i32(bytes.length),HostText::ToolVersion);}
    release_tool_bytes(bytes);
    if(version.status!=i32(0)){
        tools.failure=MsvcDiscovery::VersionText;tools.status=u32(version.status);return;
    }
    UnitSpan root=join_tool_literal(installation,"VC/Tools/MSVC");
    UnitSpan vc=empty_tool_path();
    if(address_bits(root.data)!=0u64){vc=join_tool_path(root,decoded_units(version));release_tool_path(root);}
    if(address_bits(version.data)!=0u64){release_host_wide(version.data);}
    if(address_bits(vc.data)==0u64){fail_tool_allocation(tools);return;}
    tools.paths[0]=join_tool_literal(vc,"bin/Hostx64/x64/link.exe");
    tools.paths[1]=join_tool_literal(vc,"lib/x64");
    release_tool_path(vc);
    if(address_bits(tools.paths[0].data)==0u64 || address_bits(tools.paths[1].data)==0u64){
        fail_tool_allocation(tools);
    }
}

void locate_sdk_libraries(inout MsvcTools tools) on cpu {
    HostWide installed=read_sdk_root();
    if(installed.status!=i32(0)){
        tools.failure=MsvcDiscovery::SdkRoot;tools.status=u32(installed.status);return;
    }
    UnitSpan lib=join_tool_literal(decoded_units(installed),"Lib");
    release_host_wide(installed.data);
    if(address_bits(lib.data)==0u64){fail_tool_allocation(tools);return;}
    UnitSpan pattern=join_tool_literal(lib,"*");
    if(address_bits(pattern.data)==0u64){release_tool_path(lib);fail_tool_allocation(tools);return;}
    if(tool_path_has_nul(pattern)){
        release_tool_path(pattern);release_tool_path(lib);tools.failure=MsvcDiscovery::InvalidPath;return;
    }
    array<u16,260> version=array<u16,260>(u16(0));
    SdkVersionResult selected=find_sdk_version(pattern.data,pattern.length,address_of(version[0]));
    release_tool_path(pattern);
    if(selected.status!=0 || selected.units==0){
        release_tool_path(lib);
        tools.failure=selected.status!=0 ? MsvcDiscovery::System : MsvcDiscovery::MissingSdk;
        tools.status=selected.status;
        return;
    }
    UnitSpan sdk=join_tool_path(lib,UnitSpan(address_of(version[0]),u64(selected.units)));
    release_tool_path(lib);
    if(address_bits(sdk.data)==0u64){fail_tool_allocation(tools);return;}
    tools.paths[2]=join_tool_literal(sdk,"um/x64");
    tools.paths[3]=join_tool_literal(sdk,"ucrt/x64");
    release_tool_path(sdk);
    if(address_bits(tools.paths[2].data)==0u64 || address_bits(tools.paths[3].data)==0u64){
        fail_tool_allocation(tools);
    }
}

extern "C" MsvcTools discover_msvc_tools(UnitSpan program_files) on cpu {
    SourceBytes empty=SourceBytes(address(),0u64,StorageError(0,0));
    MsvcTools tools=MsvcTools(array<UnitSpan,4>(empty_tool_path()),HostProcessOutput(empty,empty,0,0),MsvcDiscovery::Found,0);
    UnitSpan installer=join_tool_literal(program_files,"Microsoft Visual Studio/Installer/vswhere.exe");
    if(address_bits(installer.data)==0u64){fail_tool_allocation(tools);return tools;}
    HostProcessOutput process=run_vswhere(installer,
        "-utf8|-latest|-products|*|-requires|Microsoft.VisualStudio.Component.VC.Tools.x86.x64|-property|installationPath|");
    release_tool_path(installer);
    if(process.status!=0 || process.exit_code!=0 || process.stdout.error.kind!=0 || process.stderr.error.kind!=0){
        tools.failure=MsvcDiscovery::Locator;tools.output=process;return tools;
    }
    if(process.stdout.length>2147483647u64){
        release_tool_bytes(process.stdout);release_tool_bytes(process.stderr);
        tools.failure=MsvcDiscovery::TextLength;return tools;
    }
    if(process.stdout.length==0u64){
        release_tool_bytes(process.stdout);release_tool_bytes(process.stderr);
        tools.failure=MsvcDiscovery::MissingTools;return tools;
    }
    HostWide installation=decode_host_text(process.stdout.data,i32(process.stdout.length),HostText::InstallationPath);
    release_tool_bytes(process.stdout);release_tool_bytes(process.stderr);
    if(installation.status!=i32(0)){
        tools.failure=MsvcDiscovery::System;tools.status=u32(installation.status);return tools;
    }
    if(installation.units==0){
        release_host_wide(installation.data);tools.failure=MsvcDiscovery::MissingTools;return tools;
    }
    locate_vc_tools(decoded_units(installation),tools);
    release_host_wide(installation.data);
    if(tools.failure==MsvcDiscovery::Found){locate_sdk_libraries(tools);}
    return tools;
}
