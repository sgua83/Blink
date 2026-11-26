module bootstrap.cli;

enum LinkCommand:u32 {
    Link=0;
    Archive=1;
    HostArchive=2;
    StackReport=3;
    Help=4;
    Version=5;
};

enum LinkArgumentError:u32 {
    None=0;
    ProductConflict=1;
    MissingBudget=2;
    BudgetText=3;
    EmptyBudget=4;
    InvalidBudget=5;
    BudgetOverflow=6;
    MissingOutputOperand=7;
    BudgetWithoutReport=8;
    MissingOutput=9;
};

struct LinkCommandLine {
    LinkCommand command;
    LinkArgumentError error;
    u64 output_index;
    u64 budget;
    u64 has_budget;
};

struct LinkBudget {
    u64 bytes;
    LinkArgumentError error;
};

CliArgument argument_at(address arguments, u64 index) on cpu {
    return unsafe_position<CliArgument>(unsafe_address(address_bits(arguments) + index * size_of<CliArgument>()));
}

u8 argument_byte(CliArgument argument, u64 index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(argument.bytes) + index));
}

extern "C" void run_object_command(address command, address imports, u64 count,
    address context, ObjectCommandHost host) on cpu
    requires nonzero(host.import_interface) requires nonzero(host.diagnostic) requires nonzero(host.compile) {
    // Imports preceding a syntax error keep their original I/O order.
    u64 cursor = 0u64;
    while (cursor < count) {
        address item = unsafe_address(address_bits(imports) + cursor * size_of<u64>());
        if (host.import_interface(context, unsafe_position<u64>(item)) == 0) { return; }
        cursor += 1u64;
    }
    CompileCommandLine line = unsafe_position<CompileCommandLine>(command);
    if (line.error != CompileError::None) {
        host.diagnostic(context, line.error, line.error_index);
        return;
    }
    host.compile(context, command);
}

extern "C" void finish_object_command(address command, u64 arguments, address context,
    cfn<u32,address,u64,u64,u64> report, cfn<void,address> complete) on cpu
    requires nonzero(report) requires nonzero(complete) {
    CompileCommandLine line = unsafe_position<CompileCommandLine>(command);
    if (line.report_index != arguments
        && report(context, line.report_index, line.budget, line.has_budget) == 0) { return; }
    complete(context);
}

void export_byte(inout ExportSpan output, u8 value) on cpu {
    unsafe_position<u8>(unsafe_address(address_bits(output.data) + output.length)) = value;
    output.length += 1u64;
}

void export_ascii<const N:u32 in {4,5,6,7,8,9,10,13,14}>(inout ExportSpan output, read array<u8,N> text) on cpu {
    for (index in 0..N) { export_byte(output, text[index]); }
}

void export_text(inout ExportSpan output, ExportSpan text) on cpu {
    u64 index = 0u64;
    while (index < text.length) {
        export_byte(output, unsafe_position<u8>(unsafe_address(address_bits(text.data) + index)));
        index += 1u64;
    }
}

void export_number(inout ExportSpan output, u64 number) on cpu {
    u64 digits = 1u64;
    u64 remaining = number / 10u64;
    while (remaining != 0u64) {
        digits += 1u64;
        remaining /= 10u64;
    }
    u64 end = output.length + digits;
    u64 next = end;
    remaining = number;
    while (next > output.length) {
        next -= 1u64;
        unsafe_position<u8>(unsafe_address(address_bits(output.data) + next)) = u8(48u64 + remaining % 10u64);
        remaining /= 10u64;
    }
    output.length = end;
}

void export_stem(inout ExportSpan output, ExportSpan name, u64 ordinal) on cpu {
    CliArgument input = CliArgument(name.data, name.length, 1u64);
    u64 source = 0u64;
    while (source < name.length) {
        u8 byte = argument_byte(input, source);
        if (byte == 60) { break; }
        source += 1u64;
        if (byte == 58 && source < name.length && argument_byte(input, source) == 58) {
            source += 1u64;
            byte = u8(95);
        } else if (byte == 46) {
            byte = u8(95);
        }
        export_byte(output, byte);
    }
    if (source != name.length) {
        export_ascii(output, ".instance-");
        export_number(output, ordinal);
    }
}

ExportSpan export_stage_name(ExportSpan stem, u64 stage) on cpu {
    ExportSpan name = stem;
    export_byte(name, u8(46));
    export_number(name, stage);
    return name;
}

ExportSpan export_key_name(ExportSpan stem, u64 key) on cpu {
    ExportSpan name = stem;
    export_ascii(name, ".key-");
    export_number(name, key);
    return name;
}

bool export_shader(ExportHost host, ExportSpan stem, ExportSpan words) on cpu
    requires nonzero(host.prepare_spirv) requires nonzero(host.write) {
    if (!host.prepare_spirv(host.context, words)) { return false; }
    ExportSpan name = stem;
    export_ascii(name, ".spv");
    if (!host.write(host.context, name, ExportContent::Spirv, words)) { return false; }
    export_ascii(name, ".json");
    return host.write(host.context, name, ExportContent::SpirvJson, words);
}

bool export_named_shader<const N:u32 in {4,5,7,8,9,10,13,14}>(ExportHost host, ExportSpan stem, read array<u8,N> suffix, ExportSpan words) on cpu
    requires nonzero(host.prepare_spirv) requires nonzero(host.write) {
    ExportSpan name = stem;
    export_ascii(name, suffix);
    return export_shader(host, name, words);
}

bool export_optional_shader<const N:u32 in {4,5,9,10,13,14}>(ExportHost host, ExportSpan stem, read array<u8,N> suffix, ExportSpan words) on cpu
    requires nonzero(host.prepare_spirv) requires nonzero(host.write) {
    return address_bits(words.data) == 0u64 || export_named_shader(host, stem, suffix, words);
}

bool export_gpu_sort(ExportHost host, ExportSpan name, ExportSpan sort) on cpu
    requires nonzero(host.gpu_sort_shader) requires nonzero(host.prepare_spirv) requires nonzero(host.write) {
    if (sort.length == 0u64) { return true; }
    if (!export_named_shader(host, name, ".scan", host.gpu_sort_shader(sort, 16u64))) { return false; }
    if (!export_named_shader(host, name, ".tail", host.gpu_sort_shader(sort, 17u64))) { return false; }
    if (!export_named_shader(host, name, ".add", host.gpu_sort_shader(sort, 18u64))) { return false; }
    if (!export_named_shader(host, name, ".gather", host.gpu_sort_shader(sort, 19u64))) { return false; }
    u64 index = 20u64;
    while (index < sort.length) {
        if (!export_shader(host, export_key_name(name, index - 20u64), host.gpu_sort_shader(sort, index))) { return false; }
        index += 1u64;
    }
    u64 pass = 0u64;
    while (pass < 8u64) {
        ExportSpan shift = export_stage_name(name, pass * 4u64);
        if (!export_named_shader(host, shift, ".histogram", host.gpu_sort_shader(sort, pass))) { return false; }
        if (!export_named_shader(host, shift, ".scatter", host.gpu_sort_shader(sort, 8u64 + pass))) { return false; }
        pass += 1u64;
    }
    return true;
}

extern "C" bool export_gpu_image(ExportHost host, ExportSpan stem, address image, u64 count) on cpu
    requires nonzero(host.gpu_stage) requires nonzero(host.gpu_additional) requires nonzero(host.gpu_sort_shader)
    requires nonzero(host.prepare_spirv) requires nonzero(host.write) {
    u64 index = 0u64;
    while (index < count) {
        ExportGpuStage stage = host.gpu_stage(host.context, image, index);
        ExportSpan name = export_stage_name(stem, index);
        if (!export_shader(host, name, stage.main)) { return false; }
        if (!export_optional_shader(host, name, ".tail", stage.tail)) { return false; }
        if (!export_optional_shader(host, name, ".add", stage.scan_add)) { return false; }
        if (!export_optional_shader(host, name, ".dispatch", stage.dispatch)) { return false; }
        if (!export_optional_shader(host, name, ".selection", stage.selector)) { return false; }
        if (address_bits(stage.finish.data) != 0u64) {
            ExportSpan finish = name;
            export_byte(finish, u8(46));
            export_text(finish, stage.finish_name);
            if (!export_shader(host, finish, stage.finish)) { return false; }
        }

        ExportGpuAdditional extra = host.gpu_additional(image, index);
        if (!export_gpu_sort(host, name, extra.sort)) { return false; }
        if (!export_optional_shader(host, name, ".single-group", extra.segmented_single_group)) { return false; }
        if (!export_optional_shader(host, name, ".terminal-root", extra.terminal_root)) { return false; }
        if (!export_optional_shader(host, name, ".terminal-tail", extra.terminal_tail)) { return false; }
        index += 1u64;
    }
    return true;
}

bool export_native(ExportHost host, ExportSpan stem, address code, bool relocations) on cpu
    requires nonzero(host.write) {
    ExportSpan data = ExportSpan(code, 0u64);
    ExportSpan name = stem;
    export_ascii(name, ".x64");
    if (!host.write(host.context, name, ExportContent::Native, data)) { return false; }
    export_ascii(name, ".json");
    if (!host.write(host.context, name, ExportContent::NativeJson, data)) { return false; }
    if (!relocations) { return true; }
    name.length = stem.length;
    export_ascii(name, ".reloc");
    return host.write(host.context, name, ExportContent::Relocations, data);
}

bool export_named_native<const N:u32 in {4,5,7,8,10}>(ExportHost host, ExportSpan stem, read array<u8,N> suffix,
    address code, bool relocations) on cpu requires nonzero(host.write) {
    ExportSpan name = stem;
    export_ascii(name, suffix);
    return export_native(host, name, code, relocations);
}

bool export_cpu_sort(ExportHost host, ExportSpan name, ExportCpuStage stage) on cpu
    requires nonzero(host.cpu_initializer) requires nonzero(host.write) {
    if (!export_native(host, name, host.cpu_initializer(stage.initialize, 0u64), true)) { return false; }
    u64 remaining = stage.initialize.length - 1u64;
    u64 key = 0u64;
    while (remaining != 0u64) {
        if (!export_native(host, export_key_name(name, key), host.cpu_initializer(stage.initialize, remaining), false)) { return false; }
        remaining -= 1u64;
        key += 1u64;
    }
    return export_named_native(host, name, ".histogram", stage.histogram, false)
        && export_named_native(host, name, ".scatter", stage.scatter, false)
        && export_named_native(host, name, ".gather", stage.gather, false);
}

bool export_cpu_element(ExportHost host, ExportSpan name, ExportCpuStage stage) on cpu
    requires nonzero(host.write) {
    if (!export_native(host, name, stage.main, true)) { return false; }
    if (stage.continuation == ExportContinuation::ReductionTail) {
        return export_named_native(host, name, ".tail", stage.first, true);
    }
    if (stage.continuation == ExportContinuation::ScanAdd) {
        return export_named_native(host, name, ".add", stage.first, false);
    }
    if (stage.continuation == ExportContinuation::CompactScatter) {
        return export_named_native(host, name, ".scatter", stage.first, false);
    }
    if (stage.continuation == ExportContinuation::SegmentedFinish) {
        return export_named_native(host, name, ".finish", stage.first, false);
    }
    if (stage.continuation == ExportContinuation::Scan) {
        if (!export_named_native(host, name, ".prefix", stage.first, true)) { return false; }
        if (stage.custom_scan) { return export_named_native(host, name, ".add", stage.second, true); }
        return export_named_native(host, name, ".finish", stage.second, true);
    }
    return true;
}

extern "C" bool export_cpu_image(ExportHost host, ExportSpan stem, address image, u64 count) on cpu
    requires nonzero(host.cpu_stage) requires nonzero(host.cpu_initializer) requires nonzero(host.write) {
    u64 index = 0u64;
    while (index < count) {
        ExportCpuStage stage = host.cpu_stage(image, index);
        ExportSpan name = export_stage_name(stem, index);
        if (address_bits(stage.initialize.data) != 0u64) {
            if (!export_cpu_sort(host, name, stage)) { return false; }
        } else if (!export_cpu_element(host, name, stage)) {
            return false;
        }
        index += 1u64;
    }
    return true;
}

extern "C" void export_program(ExportTarget target, ExportSpan name, u64 ordinal, address output, ExportHost host) on cpu
    requires nonzero(host.gpu) requires nonzero(host.cpu) requires nonzero(host.write) {
    ExportSpan stem = ExportSpan(output, 0u64);
    export_stem(stem, name, ordinal);
    if (target != ExportTarget::Cpu && !host.gpu(host.context, stem)) { return; }
    if (target != ExportTarget::Gpu && !host.cpu(host.context, stem)) { return; }
    ExportSpan file = stem;
    ExportSpan empty = ExportSpan(unsafe_address(0u64), 0u64);
    export_ascii(file, ".layout");
    if (host.write(host.context, file, ExportContent::Layout, empty)) {
        export_ascii(file, ".json");
        host.write(host.context, file, ExportContent::LayoutJson, empty);
    }
}

bool is_token<const N:u32 in {2,3,4,5,6,7,8,9,10,14,21}>(CliArgument argument, read array<u8,N> token) on cpu {
    if (argument.length != u64(N)) { return false; }
    u32 index = 0;
    while (index < N) {
        if (argument_byte(argument, u64(index)) != token[index]) { return false; }
        index += 1;
    }
    return true;
}

LinkCommand product(CliArgument argument) on cpu {
    if (is_token(argument, "--archive")) { return LinkCommand::Archive; }
    if (is_token(argument, "--host-archive")) { return LinkCommand::HostArchive; }
    if (is_token(argument, "--stack-report")) { return LinkCommand::StackReport; }
    return LinkCommand::Link;
}


LinkBudget parse_budget(CliArgument argument) on cpu {
    if (argument.text == 0u64) { return LinkBudget(0u64, LinkArgumentError::BudgetText); }
    ParsedNumber number = unsigned_number<10>(argument.bytes, 0u64, argument.length, 18446744073709551615u64);
    if (number.error == NumberError::Empty) { return LinkBudget(0u64, LinkArgumentError::EmptyBudget); }
    if (number.error == NumberError::Invalid) { return LinkBudget(0u64, LinkArgumentError::InvalidBudget); }
    if (number.error == NumberError::Overflow) { return LinkBudget(0u64, LinkArgumentError::BudgetOverflow); }
    return LinkBudget(number.value, LinkArgumentError::None);
}

LinkCommandLine parse_link_arguments(address arguments, u64 count, address context, cfn<void,address,u64> append_input) on cpu
    requires nonzero(append_input) {
    LinkCommandLine result = LinkCommandLine(LinkCommand::Link, LinkArgumentError::None, count, 0u64, 0u64);
    if (count == 0u64) {
        result.command = LinkCommand::Help;
        return result;
    }
    CliArgument first = argument_at(arguments, 0u64);
    if (count == 1u64 && is_token(first, "--version")) {
        result.command = LinkCommand::Version;
        return result;
    }
    if (is_token(first, "--help")) {
        result.command = LinkCommand::Help;
        return result;
    }

    u64 index = 0u64;
    while (index < count) {
        CliArgument argument = argument_at(arguments, index);
        LinkCommand requested = product(argument);
        if (requested != LinkCommand::Link) {
            if (result.command != LinkCommand::Link && result.command != requested) {
                result.error = LinkArgumentError::ProductConflict;
                return result;
            }
            result.command = requested;
        } else if (is_token(argument, "--stack-budget")) {
            index += 1u64;
            if (index == count) {
                result.error = LinkArgumentError::MissingBudget;
                return result;
            }
            LinkBudget budget = parse_budget(argument_at(arguments, index));
            if (budget.error != LinkArgumentError::None) {
                result.error = budget.error;
                return result;
            }
            result.budget = budget.bytes;
            result.has_budget = 1u64;
        } else if (is_token(argument, "-o")) {
            index += 1u64;
            if (index == count) {
                result.error = LinkArgumentError::MissingOutputOperand;
                return result;
            }
            result.output_index = index;
        } else {
            append_input(context, index);
        }
        index += 1u64;
    }
    if (result.has_budget != 0u64 && result.command != LinkCommand::StackReport) {
        result.error = LinkArgumentError::BudgetWithoutReport;
    } else if (result.output_index == count) {
        result.error = LinkArgumentError::MissingOutput;
    }
    return result;
}

void command_message<const N:u32>(address context, cfn<void,address,address,u64> emit, read array<u8,N> text) on cpu
    requires nonzero(emit) {
    emit(context, address_of(text[0]), u64(N));
}

void link_error(LinkArgumentError error, LinkCommandHost host) on cpu
    requires nonzero(host.diagnostic) {
    if (error == LinkArgumentError::ProductConflict) {
        command_message(host.context, host.diagnostic, "blink只能选择一种产物命令");
    }
    else if (error == LinkArgumentError::MissingBudget) {
        command_message(host.context, host.diagnostic, "--stack-budget缺少字节数");
    }
    else if (error == LinkArgumentError::BudgetText) {
        command_message(host.context, host.diagnostic, "--stack-budget需要十进制字节数");
    }
    else if (error == LinkArgumentError::EmptyBudget) {
        command_message(host.context, host.diagnostic, "cannot parse integer from empty string");
    }
    else if (error == LinkArgumentError::InvalidBudget) {
        command_message(host.context, host.diagnostic, "invalid digit found in string");
    }
    else if (error == LinkArgumentError::BudgetOverflow) {
        command_message(host.context, host.diagnostic, "number too large to fit in target type");
    }
    else if (error == LinkArgumentError::MissingOutputOperand) {
        command_message(host.context, host.diagnostic, "-o 缺少输出路径");
    }
    else if (error == LinkArgumentError::BudgetWithoutReport) {
        command_message(host.context, host.diagnostic, "--stack-budget只用于--stack-report命令");
    }
    else if (error == LinkArgumentError::MissingOutput) {
        command_message(host.context, host.diagnostic, "用法：blink <input.obj|library.lib>... -o <output.exe|output.dll>；B接口库使用 --archive，原生宿主库使用 --host-archive");
    }
}

void link_complete<const N:u32>(LinkCommandHost host, u64 output, read array<u8,N> label) on cpu
    requires nonzero(host.complete) {
    host.complete(host.context, output, address_of(label[0]), u64(N));
}

extern "C" void run_link_command(address arguments, u64 count, LinkCommandHost host) on cpu
    requires nonzero(host.append_input) requires nonzero(host.diagnostic) requires nonzero(host.help)
    requires nonzero(host.version) requires nonzero(host.link) requires nonzero(host.archive)
    requires nonzero(host.host_archive) requires nonzero(host.stack_report) requires nonzero(host.complete) {
    LinkCommandLine line = parse_link_arguments(arguments, count, host.context, host.append_input);
    if (line.error != LinkArgumentError::None) {
        link_error(line.error, host);
    } else if (line.command == LinkCommand::Help) {
        command_message(host.context, host.help, "Blink linker\nblink --version\nblink <input.obj|library.lib>... -o <output.exe|output.dll>\nblink --archive <module.obj|library.lib>... -o <library.lib>\nblink --host-archive <module.obj|library.lib>... -o <host.lib>\nblink --stack-report <application.obj|library.lib>... -o <report.txt> [--stack-budget bytes]\n");
    } else if (line.command == LinkCommand::Version) {
        host.version(host.context);
    } else if (line.command == LinkCommand::StackReport) {
        if (host.stack_report(host.context, line.output_index, line.budget, line.has_budget) != 0) {
            link_complete(host, line.output_index, "stack report");
        }
    } else if (line.command == LinkCommand::Archive) {
        if (host.archive(host.context, line.output_index) != 0) {
            link_complete(host, line.output_index, "archived");
        }
    } else if (line.command == LinkCommand::HostArchive) {
        if (host.host_archive(host.context, line.output_index) != 0) {
            link_complete(host, line.output_index, "host archived");
        }
    } else if (host.link(host.context, line.output_index) != 0) {
        link_complete(host, line.output_index, "linked");
    }
}

extern "C" void render_compile_error(CompileError error, u64 index, CompileDiagnosticHost host) on cpu
    requires nonzero(host.text) requires nonzero(host.argument) {
    if (error == CompileError::MissingResourceOperand) {
        host.argument(host.context, index);
        command_message(host.context, host.text, "缺少参数");
    } else if (error == CompileError::ResourceText) {
        command_message(host.context, host.text, "编译资源参数不是UTF-8文本");
    } else if (error == CompileError::DuplicateJobs) {
        command_message(host.context, host.text, "--jobs不能重复指定");
    } else if (error == CompileError::DuplicateCpus) {
        command_message(host.context, host.text, "--cpus不能重复指定");
    } else if (error == CompileError::DuplicateParameterLimit) {
        command_message(host.context, host.text, "--max-parameter-bytes不能重复指定");
    } else if (error == CompileError::EmptyNumber) {
        command_message(host.context, host.text, "cannot parse integer from empty string");
    } else if (error == CompileError::InvalidNumber) {
        command_message(host.context, host.text, "invalid digit found in string");
    } else if (error == CompileError::Overflow) {
        command_message(host.context, host.text, "number too large to fit in target type");
    } else if (error == CompileError::ZeroJobs) {
        command_message(host.context, host.text, "number would be zero for non-zero type");
    } else if (error == CompileError::CpuSyntax) {
        command_message(host.context, host.text, "CPU写为组:编号");
    } else if (error == CompileError::ResourceConflict) {
        command_message(host.context, host.text, "--jobs与--cpus选择一种编译资源声明");
    } else if (error == CompileError::InputsUsage) {
        command_message(host.context, host.text, "用法：bc inputs <compiled.obj|entry.blbi>");
    } else if (error == CompileError::CommandText) {
        command_message(host.context, host.text, "命令不是UTF-8文本");
    } else if (error == CompileError::MissingSource) {
        command_message(host.context, host.text, "缺少 .b 文件");
    } else if (error == CompileError::UnknownCommand) {
        command_message(host.context, host.text, "未知命令 ");
        host.argument(host.context, index);
        command_message(host.context, host.text, "；编译使用 bc -c，链接使用 blink");
    } else if (error == CompileError::MissingOptionOperand) {
        command_message(host.context, host.text, "编译选项缺少参数");
    } else if (error == CompileError::OptionText) {
        command_message(host.context, host.text, "编译选项不是UTF-8文本");
    } else if (error == CompileError::UnknownOption) {
        command_message(host.context, host.text, "未知编译选项 ");
        host.argument(host.context, index);
    } else if (error == CompileError::BudgetText) {
        command_message(host.context, host.text, "栈预算不是UTF-8文本");
    } else if (error == CompileError::EntryText) {
        command_message(host.context, host.text, "入口不是UTF-8文本");
    } else if (error == CompileError::InstanceText) {
        command_message(host.context, host.text, "实例不是UTF-8文本");
    } else if (error == CompileError::BudgetWithoutReport) {
        command_message(host.context, host.text, "--stack-budget需要--stack-report输出");
    } else if (error == CompileError::MissingOutput) {
        command_message(host.context, host.text, "缺少 -o 目标文件路径");
    } else if (error == CompileError::CodeUsage) {
        command_message(host.context, host.text, "usage: bc code <entry.blbi> [--observations <ranges.blcr>] [--source-range start:end] | bc code --image <deployment.blri> [--observations <ranges.blcr>] [--source-range start:end]");
    } else if (error == CompileError::ObjectCodeUsage) {
        command_message(host.context, host.text, "usage: bc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol] [--source-range start:end]");
    } else if (error == CompileError::IrUsage || error == CompileError::IrOption) {
        if (error == CompileError::IrOption) {
            command_message(host.context, host.text, "未知IR选项 ");
            host.argument(host.context, index);
            command_message(host.context, host.text, "；");
        }
        command_message(host.context, host.text, "用法：bc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]");
    } else if (error == CompileError::IrModeRepeated) {
        command_message(host.context, host.text, "IR编译模式只能指定一次");
    } else if (error == CompileError::IrFunctionRepeated) {
        command_message(host.context, host.text, "--function不能重复指定");
    } else if (error == CompileError::IrStageRepeated) {
        command_message(host.context, host.text, "--stage不能重复指定");
    } else if (error == CompileError::IrFunctionMode) {
        command_message(host.context, host.text, "--function用于普通应用／模块，--stage用于--program");
    } else if (error == CompileError::IrStageMode) {
        command_message(host.context, host.text, "--stage需要--program");
    } else if (error == CompileError::IrInstanceMode) {
        command_message(host.context, host.text, "--instance用于模块；入口实例直接写在--entry／--program值中");
    } else if (error == CompileError::LayoutUsage || error == CompileError::LayoutOption) {
        if (error == CompileError::LayoutOption) {
            command_message(host.context, host.text, "未知布局选项 ");
            host.argument(host.context, index);
            command_message(host.context, host.text, "；");
        }
        command_message(host.context, host.text, "用法：bc layout <module.h|module.b> <type> [-I module-root] [--field path]");
    } else if (error == CompileError::X64Usage || error == CompileError::X64Option) {
        if (error == CompileError::X64Option) {
            command_message(host.context, host.text, "未知x64查询选项 ");
            host.argument(host.context, index);
            command_message(host.context, host.text, "；");
        }
        command_message(host.context, host.text, "用法：bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]");
    } else if (error == CompileError::X64Repeated) {
        host.argument(host.context, index);
        command_message(host.context, host.text, "不能重复指定");
    } else if (error == CompileError::ImageUsage) {
        command_message(host.context, host.text, "用法：bc image <entry.blbi> -o <deployment.blri>");
    } else if (error == CompileError::SpirvUsage) {
        command_message(host.context, host.text, "usage: bc spirv <module.spv>");
    }
}

extern "C" void write_compile_help(address context, cfn<void,address,address,u64> emit) on cpu
    requires nonzero(emit) {
    command_message(context, emit, "Blink compiler\nbc --version\nbc inputs <compiled.obj|entry.blbi>\nbc image <entry.blbi> -o <deployment.blri>\nbc code <entry.blbi> [--observations <ranges.blcr>]\nbc code --image <deployment.blri> [--observations <ranges.blcr>]\nbc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol]\nbc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]\nbc spirv <module.spv>\nbc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]\nbc layout <module.h|module.b> <type> [-I module-root] [--field path]\nGlobal compile options before command: [--jobs N | --cpus group:number,...] [--max-parameter-bytes N]\nbc -c <module.b> -o <output.obj> [-I module-root] [--entry main | --program name | --module] [--import module.obj|library.lib] [--instance function<args>] [--stack-report report.txt [--stack-budget bytes]]\nbc check <module.b> [module-root] [program<args>]\nbc build <module.b> <output-directory> [module-root] [program<args>]\nLink separately: blink <input.obj> -o <output.exe|output.dll>\n");
}

enum ResourceOption:u32 { None=0; Jobs=1; Cpus=2; ParameterLimit=3; };

CompileError compile_number_error(NumberError error) on cpu {
    if (error == NumberError::Empty) { return CompileError::EmptyNumber; }
    if (error == NumberError::Invalid) { return CompileError::InvalidNumber; }
    if (error == NumberError::Overflow) { return CompileError::Overflow; }
    return CompileError::None;
}

ResourceOption resource_option(CliArgument argument) on cpu {
    if (is_token(argument, "--jobs")) { return ResourceOption::Jobs; }
    if (is_token(argument, "--cpus")) { return ResourceOption::Cpus; }
    if (is_token(argument, "--max-parameter-bytes")) { return ResourceOption::ParameterLimit; }
    return ResourceOption::None;
}

CompileError parse_cpus(CliArgument argument, OperandSink operands) on cpu
    requires nonzero(operands.processor) {
    u64 begin = 0u64;
    while (begin <= argument.length) {
        u64 end = begin;
        u64 colon = argument.length;
        while (end < argument.length) {
            u8 byte = argument_byte(argument, end);
            if (byte == 44) { break; }
            if (byte == 58 && colon == argument.length) { colon = end; }
            end += 1u64;
        }
        if (colon == argument.length) { return CompileError::CpuSyntax; }
        ParsedNumber group = unsigned_number<10>(argument.bytes, begin, colon, 65535u64);
        if (group.error != NumberError::None) { return compile_number_error(group.error); }
        ParsedNumber number = unsigned_number<10>(argument.bytes, colon + 1u64, end, 255u64);
        if (number.error != NumberError::None) { return compile_number_error(number.error); }
        operands.processor(operands.context, u16(group.value), u8(number.value));
        begin = end + 1u64;
    }
    return CompileError::None;
}

CompileCommand early_command(CliArgument argument) on cpu {
    if (is_token(argument, "inputs")) { return CompileCommand::Inputs; }
    if (is_token(argument, "x64")) { return CompileCommand::X64; }
    if (is_token(argument, "image")) { return CompileCommand::Image; }
    if (is_token(argument, "code")) { return CompileCommand::Code; }
    if (is_token(argument, "spirv")) { return CompileCommand::Spirv; }
    return CompileCommand::Unknown;
}

CompileCommand compiler_command(CliArgument argument) on cpu {
    if (is_token(argument, "layout")) { return CompileCommand::Layout; }
    if (is_token(argument, "ir")) { return CompileCommand::Ir; }
    if (is_token(argument, "-c")) { return CompileCommand::Object; }
    if (is_token(argument, "check")) { return CompileCommand::Check; }
    if (is_token(argument, "build")) { return CompileCommand::Build; }
    return CompileCommand::Unknown;
}

CompileCommandLine parse_object_options(address arguments, u64 count, OperandSink operands, CompileCommandLine line) on cpu
    requires nonzero(operands.import_path) requires nonzero(operands.instance_name) {
    CompileCommandLine result = line;
    u64 index = line.source_index + 1u64;
    while (index < count) {
        CliArgument option = argument_at(arguments, index);
        if (is_token(option, "--module")) {
            result.mode = EntryMode::Module;
            result.entry_index = count;
            index += 1u64;
            continue;
        }
        u64 operand = index + 1u64;
        result.error_index = index;
        if (operand == count) { result.error = CompileError::MissingOptionOperand; return result; }
        if (option.text == 0u64) { result.error = CompileError::OptionText; return result; }
        CliArgument argument = argument_at(arguments, operand);
        if (is_token(option, "-o")) { result.output_index = operand; }
        else if (is_token(option, "-I")) { result.root_index = operand; }
        else if (is_token(option, "--stack-report")) { result.report_index = operand; }
        else if (is_token(option, "--stack-budget")) {
            if (argument.text == 0u64) { result.error = CompileError::BudgetText; return result; }
            ParsedNumber budget = unsigned_number<10>(argument.bytes, 0u64, argument.length, 18446744073709551615u64);
            if (budget.error != NumberError::None) { result.error = compile_number_error(budget.error); return result; }
            result.budget = budget.value;
            result.has_budget = 1u64;
        } else if (is_token(option, "--entry") || is_token(option, "--program")) {
            if (argument.text == 0u64) { result.error = CompileError::EntryText; return result; }
            if (is_token(option, "--program")) {
                if (result.mode != EntryMode::Module) {
                    result.mode = EntryMode::Program;
                    result.entry_index = operand;
                }
            } else if (result.mode == EntryMode::Application) {
                result.entry_index = operand;
            }
        } else if (is_token(option, "--import")) {
            operands.import_path(operands.context,operand);
        } else if (is_token(option, "--instance")) {
            if (argument.text == 0u64) { result.error = CompileError::InstanceText; return result; }
            operands.instance_name(operands.context,operand);
        } else {
            result.error = CompileError::UnknownOption;
            return result;
        }
        index += 2u64;
    }
    if (result.has_budget != 0u64 && result.report_index == count) { result.error = CompileError::BudgetWithoutReport; }
    else if (result.output_index == count) { result.error = CompileError::MissingOutput; }
    return result;
}

extern "C" CompileCommandLine parse_compile_arguments(address arguments, u64 count, OperandSink operands) on cpu
    requires nonzero(operands.import_path) requires nonzero(operands.instance_name) requires nonzero(operands.processor) {
    CompileCommandLine result = CompileCommandLine(
        CompileCommand::Unknown, CompileError::None, CompileError::None, 0,
        0u64, count, count, count, count, count, count, 0u64, 0u64, EntryMode::Application, 0u64, 0u64);
    if (count == 0u64) { result.command = CompileCommand::Help; return result; }
    CliArgument first = argument_at(arguments, 0u64);
    if (count == 1u64 && is_token(first, "--version")) { result.command = CompileCommand::Version; return result; }
    result.command = early_command(first);
    if (result.command != CompileCommand::Unknown) {
        if (result.command == CompileCommand::Inputs || result.command == CompileCommand::Spirv) {
            if (count == 2u64) { result.source_index = 1u64; }
            else {
                result.error = result.command == CompileCommand::Inputs ? CompileError::InputsUsage : CompileError::SpirvUsage;
            }
        } else if (result.command == CompileCommand::Image) {
            if (count != 4u64 || !is_token(argument_at(arguments, 2u64), "-o")) {
                result.error = CompileError::ImageUsage;
            } else {
                result.source_index = 1u64;
                result.output_index = 3u64;
            }
        }
        return result;
    }

    bool has_cpus = false;
    u64 index = 0u64;
    while (index < count) {
        ResourceOption option = resource_option(argument_at(arguments, index));
        if (option == ResourceOption::None) { break; }
        result.error_index = index;
        u64 operand = index + 1u64;
        if (operand == count) { result.prefix_error = CompileError::MissingResourceOperand; return result; }
        CliArgument argument = argument_at(arguments, operand);
        if (argument.text == 0u64) { result.prefix_error = CompileError::ResourceText; return result; }
        if (option == ResourceOption::Jobs) {
            if (result.jobs != 0u64) { result.prefix_error = CompileError::DuplicateJobs; return result; }
            ParsedNumber jobs = unsigned_number<10>(argument.bytes, 0u64, argument.length, 18446744073709551615u64);
            if (jobs.error != NumberError::None) { result.prefix_error = compile_number_error(jobs.error); return result; }
            if (jobs.value == 0u64) { result.prefix_error = CompileError::ZeroJobs; return result; }
            result.jobs = jobs.value;
        } else if (option == ResourceOption::Cpus) {
            if (has_cpus) { result.prefix_error = CompileError::DuplicateCpus; return result; }
            result.prefix_error = parse_cpus(argument, operands);
            if (result.prefix_error != CompileError::None) { return result; }
            has_cpus = true;
        } else {
            if (result.has_parameter_limit != 0u64) { result.prefix_error = CompileError::DuplicateParameterLimit; return result; }
            ParsedNumber maximum = unsigned_number<10>(argument.bytes, 0u64, argument.length, 4294967295u64);
            if (maximum.error != NumberError::None) { result.prefix_error = compile_number_error(maximum.error); return result; }
            result.max_parameter_bytes = u32(maximum.value);
            result.has_parameter_limit = 1u64;
        }
        index += 2u64;
    }
    if (result.jobs != 0u64 && has_cpus) { result.prefix_error = CompileError::ResourceConflict; return result; }
    result.command_index = index;
    if (index == count) { result.command = CompileCommand::Help; return result; }
    CliArgument command = argument_at(arguments, index);
    if (is_token(command, "--help")) { result.command = CompileCommand::Help; return result; }
    if (command.text == 0u64) { result.error = CompileError::CommandText; return result; }
    result.command = compiler_command(command);
    if (result.command == CompileCommand::Layout || result.command == CompileCommand::Ir) { return result; }
    if (index + 1u64 == count) { result.command = CompileCommand::Unknown; result.error = CompileError::MissingSource; return result; }
    result.source_index = index + 1u64;
    if (result.command == CompileCommand::Object) { return parse_object_options(arguments, count, operands, result); }
    if (result.command == CompileCommand::Unknown) { result.error = CompileError::UnknownCommand; result.error_index = index; return result; }

    u64 root = index + 2u64;
    if (result.command == CompileCommand::Build && root < count) {
        result.output_index = root;
        root += 1u64;
    }
    if (root < count) { result.root_index = root; }
    if (root + 1u64 < count) {
        result.entry_index = root + 1u64;
        if (argument_at(arguments, result.entry_index).text == 0u64) { result.error = CompileError::EntryText; }
    }
    return result;
}

extern "C" void run_program_command(CompileCommand command, u64 arguments, u64 entry, u64 output, ProgramCommandHost host) on cpu
    requires nonzero(host.compile) requires nonzero(host.report_generics) requires nonzero(host.describe)
    requires nonzero(host.prepare_output) requires nonzero(host.export_program) requires nonzero(host.report_library) {
    bool build = command == CompileCommand::Build;
    ProgramRequest request = entry < arguments ? ProgramRequest::Entry : (build ? ProgramRequest::All : ProgramRequest::Check);
    ProgramResults programs = host.compile(host.context, request, entry);
    if (programs.success == 0) { return; }
    if (programs.generics != 0 && host.report_generics(host.context) == 0) { return; }
    u64 index = 0u64;
    while (index < programs.count) {
        if (host.describe(host.context, index) == 0) { return; }
        index += 1u64;
    }
    if (build) {
        if (host.prepare_output(host.context, output) == 0) { return; }
        index = 0u64;
        while (index < programs.count) {
            if (host.export_program(host.context, output, index) == 0) { return; }
            index += 1u64;
        }
    } else if (programs.count == 0u64) {
        host.report_library(host.context);
    }
}

extern "C" CodeCommandLine parse_code_arguments(address arguments, u64 count) on cpu {
    CodeCommandLine line = CodeCommandLine(
        CodeInput::Entry, CompileError::CodeUsage,
        1u64, count, count, 0u64, 0u64, 0u64, false, false);
    if (count <= 1u64) { return line; }
    CliArgument input = argument_at(arguments, 1u64);
    if (is_token(input, "--object")) {
        line.input = CodeInput::Object;
        line.error = CompileError::ObjectCodeUsage;
        line.source_index = 2u64;
    } else if (is_token(input, "--image")) {
        line.input = CodeInput::Image;
        line.source_index = 2u64;
    }
    if (line.source_index == count) { return line; }

    u64 index = line.source_index + 1u64;
    while (index < count) {
        u64 operand = index + 1u64;
        if (operand == count) { return line; }
        CliArgument option = argument_at(arguments, index);
        CliArgument value = argument_at(arguments, operand);
        if (is_token(option, "--observations") && line.observations_index == count) {
            line.observations_index = operand;
        } else if (is_token(option, "--source-range") && !line.has_range) {
            if (value.text == 0u64) { return line; }
            u64 colon = 0u64;
            while (colon < value.length) {
                if (argument_byte(value, colon) == 58) { break; }
                colon += 1u64;
            }
            if (colon == value.length) { return line; }
            ParsedNumber start = unsigned_number<10>(value.bytes, 0u64, colon, 18446744073709551615u64);
            if (start.error != NumberError::None) {
                line.error = compile_number_error(start.error);
                return line;
            }
            ParsedNumber end = unsigned_number<10>(value.bytes, colon + 1u64, value.length, 18446744073709551615u64);
            if (end.error != NumberError::None) {
                line.error = compile_number_error(end.error);
                return line;
            }
            line.range_start = start.value;
            line.range_end = end.value;
            line.has_range = true;
        } else if (line.input == CodeInput::Object && !line.has_section && line.function_index == count) {
            if (value.text == 0u64) { return line; }
            if (is_token(option, "--section")) {
                ParsedNumber section = unsigned_number<10>(value.bytes, 0u64, value.length, 18446744073709551615u64);
                if (section.error != NumberError::None) {
                    line.error = compile_number_error(section.error);
                    return line;
                }
                line.section = section.value;
                line.has_section = true;
            } else if (is_token(option, "--function")) {
                line.function_index = operand;
            } else {
                return line;
            }
        } else {
            return line;
        }
        index += 2u64;
    }
    line.error = CompileError::None;
    return line;
}

extern "C" IrCommandLine parse_ir_arguments(address arguments, u64 count, OperandSink operands) on cpu
    requires nonzero(operands.import_path) requires nonzero(operands.instance_name) {
    IrCommandLine line = IrCommandLine(
        EntryMode::Module, CompileError::None, count, count, count, count, 0u64, 0u64);
    bool explicit_mode = false;
    bool has_instances = false;
    u64 index = 2u64;
    while (index < count) {
        CliArgument option = argument_at(arguments, index);
        if (is_token(option, "--module")) {
            if (explicit_mode) { line.error = CompileError::IrModeRepeated; return line; }
            explicit_mode = true;
            index += 1u64;
            continue;
        }

        u64 operand = index + 1u64;
        if (operand == count || option.text == 0u64) { line.error = CompileError::IrUsage; return line; }
        CliArgument value = argument_at(arguments, operand);
        if (is_token(option, "-I")) {
            line.root_index = operand;
        } else if (is_token(option, "--entry") || is_token(option, "--program")) {
            if (value.text == 0u64) { line.error = CompileError::IrUsage; return line; }
            if (explicit_mode) { line.error = CompileError::IrModeRepeated; return line; }
            explicit_mode = true;
            line.mode = is_token(option, "--entry") ? EntryMode::Application : EntryMode::Program;
            line.entry_index = operand;
        } else if (is_token(option, "--function")) {
            if (value.text == 0u64) { line.error = CompileError::IrUsage; return line; }
            if (line.function_index != count) { line.error = CompileError::IrFunctionRepeated; return line; }
            line.function_index = operand;
        } else if (is_token(option, "--stage")) {
            if (value.text == 0u64) { line.error = CompileError::IrUsage; return line; }
            ParsedNumber stage = unsigned_number<10>(value.bytes, 0u64, value.length, 18446744073709551615u64);
            if (stage.error != NumberError::None) {
                line.error = compile_number_error(stage.error);
                return line;
            }
            if (line.has_stage != 0u64) { line.error = CompileError::IrStageRepeated; return line; }
            line.stage = stage.value;
            line.has_stage = 1u64;
        } else if (is_token(option, "--import")) {
            operands.import_path(operands.context,operand);
        } else if (is_token(option, "--instance")) {
            if (value.text == 0u64) { line.error = CompileError::IrUsage; return line; }
            operands.instance_name(operands.context,operand);
            has_instances = true;
        } else {
            line.error = CompileError::IrOption;
            line.error_index = index;
            return line;
        }
        index += 2u64;
    }
    if (line.function_index != count && (line.has_stage != 0u64 || line.mode == EntryMode::Program)) {
        line.error = CompileError::IrFunctionMode;
    } else if (line.has_stage != 0u64 && line.mode != EntryMode::Program) {
        line.error = CompileError::IrStageMode;
    } else if (has_instances && line.mode != EntryMode::Module) {
        line.error = CompileError::IrInstanceMode;
    }
    return line;
}

extern "C" LayoutCommandLine parse_layout_arguments(address arguments, u64 count) on cpu {
    LayoutCommandLine line = LayoutCommandLine(CompileError::LayoutUsage, count, count, count);
    if (count == 2u64 || argument_at(arguments, 2u64).text == 0u64) { return line; }
    u64 index = 3u64;
    while (index < count) {
        u64 operand = index + 1u64;
        if (operand == count) { return line; }
        CliArgument option = argument_at(arguments, index);
        if (option.text == 0u64) { return line; }
        if (is_token(option, "-I")) {
            line.root_index = operand;
        } else if (is_token(option, "--field")) {
            if (argument_at(arguments, operand).text == 0u64) { return line; }
            line.field_index = operand;
        } else {
            line.error = CompileError::LayoutOption;
            line.error_index = index;
            return line;
        }
        index += 2u64;
    }
    line.error = CompileError::None;
    return line;
}

extern "C" X64CommandLine parse_x64_arguments(address arguments, u64 count) on cpu {
    X64CommandLine line = X64CommandLine(CompileError::X64Usage, count, 0u64, 0u64, 0u64, 0u64);
    bool has_ip = false;
    bool has_offset = false;
    u64 index = 2u64;
    while (index < count) {
        u64 operand = index + 1u64;
        if (operand == count) { return line; }
        CliArgument option = argument_at(arguments, index);
        if (option.text == 0u64) { return line; }
        bool ip = is_token(option, "--ip");
        bool offset = is_token(option, "--offset");
        bool length = is_token(option, "--length");
        line.error_index = index;
        if (!ip && !offset && !length) {
            line.error = CompileError::X64Option;
            return line;
        }
        CliArgument value = argument_at(arguments, operand);
        if (value.text == 0u64) { return line; }
        bool hex = value.length >= 2u64 && argument_byte(value, 0u64) == 48 && argument_byte(value, 1u64) == 120;
        ParsedNumber number = hex ? unsigned_number<16>(value.bytes, 2u64, value.length, 18446744073709551615u64) :
            unsigned_number<10>(value.bytes, 0u64, value.length, 18446744073709551615u64);
        if (number.error != NumberError::None) {
            line.error = compile_number_error(number.error);
            return line;
        }
        if ((ip && has_ip) || (offset && has_offset) || (length && line.has_length != 0u64)) {
            line.error = CompileError::X64Repeated;
            return line;
        }
        if (ip) {
            line.ip = number.value;
            has_ip = true;
        } else if (offset) {
            line.offset = number.value;
            has_offset = true;
        } else {
            line.length = number.value;
            line.has_length = 1u64;
        }
        index += 2u64;
    }
    line.error = CompileError::None;
    return line;
}

struct MsvcWriter { address data; u64 units; };

void msvc_unit(inout MsvcWriter writer, u16 value) on cpu {
    unsafe_position<u16>(unsafe_address(address_bits(writer.data) + writer.units * 2u64)) = value;
    writer.units += 1u64;
}

void msvc_ascii<const N:u32 in {4,5,6,7,8,9,12,18}>(inout MsvcWriter writer, read array<u8,N> text) on cpu {
    u32 index = 0;
    while (index < N) {
        msvc_unit(writer, u16(text[index]));
        index += 1;
    }
}

void msvc_flag<const N:u32 in {4,6,7,8,9,12,18}>(inout MsvcWriter writer, read array<u8,N> flag) on cpu {
    msvc_ascii(writer, flag);
    msvc_unit(writer, u16(0));
}

void msvc_wide(inout MsvcWriter writer, MsvcWide span) on cpu {
    u64 index = 0u64;
    while (index < span.units) {
        address unit = unsafe_address(address_bits(span.data) + index * 2u64);
        msvc_unit(writer, unsafe_position<u16>(unit));
        index += 1u64;
    }
    msvc_unit(writer, u16(0));
}

void msvc_text(inout MsvcWriter writer, address text, MsvcText span) on cpu {
    address first = unsafe_address(address_bits(text) + span.start * 2u64);
    msvc_wide(writer, MsvcWide(first, span.units));
}

MsvcWide msvc_path(MsvcGroup group, u64 index) on cpu {
    return unsafe_position<MsvcWide>(
        unsafe_address(address_bits(group.spans) + index * size_of<MsvcWide>()));
}

MsvcText msvc_span(MsvcGroup group, u64 index) on cpu {
    return unsafe_position<MsvcText>(
        unsafe_address(address_bits(group.spans) + index * size_of<MsvcText>()));
}

void msvc_decimal(inout MsvcWriter writer, u64 number) on cpu {
    u64 value = number;
    u64 scale = 10000000000000000000u64;
    bool started = false;
    while (scale != 0u64) {
        u64 digit = value / scale;
        started = started || digit != 0u64;
        if (started) { msvc_unit(writer, u16(48u64 + digit)); }
        value -= digit * scale;
        scale /= 10u64;
    }
    msvc_unit(writer, u16(0));
}

extern "C" u64 write_msvc_arguments(MsvcArguments input, address output) on cpu {
    MsvcWriter writer = MsvcWriter(output, 0u64);
    msvc_flag(writer, "/NOLOGO");
    msvc_flag(writer, "/MACHINE:X64");
    msvc_flag(writer, "/OPT:REF");
    msvc_flag(writer, "/OPT:ICF");
    msvc_flag(writer, "/NOEXP");
    msvc_ascii(writer, "/OUT:");
    msvc_wide(writer, input.output);
    u64 index = 0u64;
    while (index < input.inputs.count) {
        msvc_wide(writer, msvc_path(input.inputs, index));
        index += 1u64;
    }
    if (input.stack_reserve == 0u64) {
        msvc_flag(writer, "/DLL");
    } else {
        msvc_flag(writer, "/SUBSYSTEM:CONSOLE");
        msvc_ascii(writer, "/STACK:");
        msvc_decimal(writer, input.stack_reserve);
    }
    if (input.import_library.units == 0u64) {
        msvc_flag(writer, "/NOIMPLIB");
    } else {
        msvc_ascii(writer, "/IMPLIB:");
        msvc_wide(writer, input.import_library);
    }
    index = 0u64;
    while (index < input.search_paths.count) {
        msvc_ascii(writer, "/LIBPATH:");
        msvc_wide(writer, msvc_path(input.search_paths, index));
        index += 1u64;
    }
    index = 0u64;
    while (index < input.libraries.count) {
        msvc_text(writer, input.text, msvc_span(input.libraries, index));
        index += 1u64;
    }
    index = 0u64;
    while (index < input.exports.count) {
        msvc_ascii(writer, "/EXPORT:");
        msvc_text(writer, input.text, msvc_span(input.exports, index));
        index += 1u64;
    }
    return writer.units;
}
