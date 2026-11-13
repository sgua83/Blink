module bootstrap.host_sdk;
import blink.windows.directory;
import blink.windows.error;
import blink.windows.file;
import blink.windows.system;
import blink.windows.path_core;
import blink.windows.process;
import blink.windows.process_extended;
import blink.windows.thread;

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

struct HostFileOpen { handle file; u32 status; };

HostFileOpen open_host_file(address path, usize units, u32 access, u32 creation, u32 flags) on cpu {
    NativePath name = native_path(path, units, true);
    if (name.status != 0) { return HostFileOpen(INVALID_HANDLE_VALUE, name.status); }

    handle file = CreateFileW(name.name, access, 7, address(), creation, flags, handle());
    u32 status = file == INVALID_HANDLE_VALUE || creation == OPEN_ALWAYS ? GetLastError() : 0;
    release_native_path(name);
    return HostFileOpen(file, status);
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
    CloseHandle(file);
    return result;
}

extern "C" HostWriteResult write_host_input(address path, usize units, address data, usize bytes) on cpu {
    HostFileOpen opened = open_host_file(path, units, GENERIC_READ_WRITE, CREATE_NEW, FILE_FLAG_OPEN_REPARSE_POINT);
    if (opened.file == INVALID_HANDLE_VALUE) {
        return HostWriteResult(HostWriteKind::OpenError, opened.status);
    }
    return write_host_bytes(opened.file, data, bytes);
}

extern "C" HostWriteResult write_host_output(address path, usize units, address data, usize bytes) on cpu {
    HostFileOpen opened = open_host_file(path, units, GENERIC_WRITE, OPEN_ALWAYS, 0);
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
    return write_host_bytes(opened.file, data, bytes);
}

extern "C" u32 remove_host_input(address path, usize units) on cpu {
    HostFileOpen opened = open_host_file(path, units, DELETE_ACCESS, OPEN_EXISTING, FILE_FLAG_OPEN_REPARSE_POINT);
    if (opened.file == INVALID_HANDLE_VALUE) { return opened.status; }

    // FILE_DISPOSITION_INFO_EX: delete the file, using POSIX semantics and ignoring its read-only attribute.
    u32 disposition = 19;
    i32 removed = SetFileInformationByHandle(opened.file, i32(21), address_of(disposition), u32(size_of<u32>()));
    u32 status = removed == i32(0) ? GetLastError() : 0;
    CloseHandle(opened.file);
    return status;
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

HostChild create_host_child(address executable, address command_line, read array<handle,3> stdio) on cpu {
    usize size = 0u64;
    InitializeProcThreadAttributeList(address(), 1, 0, size);
    if (size == 0u64) { return HostChild(handle(), GetLastError()); }

    handle heap = GetProcessHeap();
    address attributes = HeapAlloc(heap, 0, size);
    if (address_bits(attributes) == 0u64) {
        return HostChild(handle(), u32(ERROR_NOT_ENOUGH_MEMORY));
    }
    if (InitializeProcThreadAttributeList(attributes, 1, 0, size) == i32(0)) {
        u32 status = GetLastError();
        HeapFree(heap, 0, attributes);
        return HostChild(handle(), status);
    }

    HostChild child = HostChild(handle(), 0);
    if (UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            address_of(stdio[0]), size_of<array<handle,3>>(), address(), address()) == i32(0)) {
        child.status = GetLastError();
    } else {
        StartupInfo startup = StartupInfo(u32(size_of<StartupInfoEx>()), address(), address(), address(),
            0, 0, 0, 0, 0, 0, 0, STARTF_USESTDHANDLES, u16(0), u16(0), address(),
            stdio[0], stdio[1], stdio[2]);
        StartupInfoEx extended = StartupInfoEx(startup, attributes);
        ProcessInformation info;
        if (blink.windows.process_extended::CreateProcessW(executable, command_line, address(), address(),
                i32(1), EXTENDED_STARTUPINFO_PRESENT | DETACHED_PROCESS, address(), address(),
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
                child = create_host_child(path.name, command_line, stdio);
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

    u32 exit_code = 0;
    u32 status = 0;
    if (WaitForSingleObject(child.process, INFINITE) == WAIT_FAILED) {
        status = GetLastError();
    } else {
        u32 code;
        if (GetExitCodeProcess(child.process, code) != i32(0)) {
            exit_code = code;
        } else {
            status = GetLastError();
        }
    }
    CloseHandle(child.process);
    return HostProcessOutput(output, work.bytes, exit_code, status);
}
