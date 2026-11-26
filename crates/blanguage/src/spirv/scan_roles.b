module spirv.scan_roles;

extern "C" u32 histogram_count(u32 count, u32 width) on cpu requires nonzero(width) {
    u32 groups = count / width + u32(count % width != 0);
    return max(groups, 1) * 16;
}

bool segmented(EntryOperation operation) on cpu {
    return operation == EntryOperation::Segmented || operation == EntryOperation::CustomSegmented;
}

extern "C" ScanRoles scan_roles(ScanShape shape) on cpu requires nonzero(shape.width) {
    EntryOperation operation = shape.operation;
    bool root = operation == EntryOperation::Sort || operation == EntryOperation::Scan
        || operation == EntryOperation::CustomScan || operation == EntryOperation::Compact
        || (operation == EntryOperation::Map && shape.selected != 0);
    if (!root && !segmented(operation)) {
        return ScanRoles(ScanShader::Summary, ScanShader::Summary);
    }
    if (shape.fixed == 0) {
        return ScanRoles(root ? ScanShader::Both : ScanShader::Summary, ScanShader::Both);
    }
    u32 count = operation == EntryOperation::Sort ? histogram_count(shape.count, shape.width) : shape.count;
    ScanShader first = root && count <= shape.width ? ScanShader::Terminal : ScanShader::Summary;
    ScanShader tail = ScanShader::Summary;
    if (count > shape.width) {
        tail = count > shape.width * shape.width ? ScanShader::Both : ScanShader::Terminal;
    }
    return ScanRoles(first, tail);
}

u32 shader_word(ShaderWords words, u64 index) on cpu {
    return unsafe_position<u32>(unsafe_address(address_bits(words.data) + index * 4u64));
}

// A terminal shader has no auxiliary page binding in its module declarations.
bool declares_binding(ShaderWords words, u32 binding) on cpu {
    if (words.count < 5u64) { return true; }
    u64 index = 5u64;
    while (index < words.count) {
        u32 header = shader_word(words, index);
        u64 width = u64(header >> 16);
        if (width > words.count - index) { return true; }
        u32 opcode = header & 65535;
        if (opcode == 54) { break; }
        if (width == 0u64) { return true; }
        if (opcode == 71 && width == 4u64
            && shader_word(words, index + 2u64) == 33
            && shader_word(words, index + 3u64) == binding) {
            return true;
        }
        index += width;
    }
    return false;
}

TerminalEntry terminal_entry(ScanShader role, bool root, StageShaders input, u64 sort, u64 next,
    address context, cfn<ShaderWords,address,u64> ensures memory(read) additional) on cpu
    requires nonzero(additional) {
    if (role == ScanShader::Both && next < input.additional_count) {
        return TerminalEntry(next, TerminalKind::Alternate);
    }
    if (role == ScanShader::Terminal) {
        ShaderWords words;
        if (sort != 0u64) { words = additional(context, root ? 16u64 : 17u64); }
        else if (!root) { words = input.tail; }
        else if (input.shape.operation == EntryOperation::Sort) { return TerminalEntry(0u64, TerminalKind::Absent); }
        else if (input.shape.operation == EntryOperation::Map && input.shape.selected != 0) { words = input.selector; }
        else { words = input.main; }
        if (!declares_binding(words, input.binding)) {
            return TerminalEntry(0u64, TerminalKind::Canonical);
        }
    }
    return TerminalEntry(0u64, TerminalKind::Absent);
}

extern "C" AdditionalEntries additional_entries(StageShaders input, address context,
    cfn<ShaderWords,address,u64> ensures memory(read) additional) on cpu
    requires nonzero(additional) requires nonzero(input.shape.width) {
    u64 next = 0u64;
    u64 sort = 0u64;
    if (input.shape.operation == EntryOperation::Sort) {
        next = 19u64 + input.sort_keys;
        if (next <= input.additional_count) { sort = next; }
    }
    u64 single_group = input.additional_count;
    if (segmented(input.shape.operation) && next < input.additional_count
        && (input.shape.fixed == 0 || input.shape.count <= input.shape.width)) {
        single_group = next;
        next += 1u64;
    }
    ScanRoles roles = scan_roles(input.shape);
    TerminalEntry root = terminal_entry(roles.root, true, input, sort, next, context, additional);
    next += u64(root.kind == TerminalKind::Alternate);
    TerminalEntry tail = terminal_entry(roles.tail, false, input, sort, next, context, additional);
    return AdditionalEntries(sort, single_group, root, tail);
}
