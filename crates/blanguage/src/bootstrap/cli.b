module bootstrap.cli;

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

void write_stem_byte(address output, u64 index, u8 value) on cpu {
    unsafe_position<u8>(unsafe_address(address_bits(output) + index)) = value;
}

extern "C" u64 write_export_stem(address name, u64 length, u64 ordinal, address output) on cpu {
    CliArgument input = CliArgument(name, length, 1u64);
    u64 source = 0u64;
    u64 written = 0u64;
    while (source < length) {
        u8 byte = argument_byte(input, source);
        if (byte == 60) { break; }
        source += 1u64;
        if (byte == 58 && source < length && argument_byte(input, source) == 58) {
            source += 1u64;
            byte = u8(95);
        } else if (byte == 46) {
            byte = u8(95);
        }
        write_stem_byte(output, written, byte);
        written += 1u64;
    }
    if (source == length) { return written; }

    array<u8,10> suffix = ".instance-";
    for (index in 0..10) {
        write_stem_byte(output, written, suffix[index]);
        written += 1u64;
    }

    u64 digits = 1u64;
    u64 remaining = ordinal / 10u64;
    while (remaining != 0u64) {
        digits += 1u64;
        remaining /= 10u64;
    }
    u64 end = written + digits;
    u64 next = end;
    remaining = ordinal;
    while (next > written) {
        next -= 1u64;
        write_stem_byte(output, next, u8(48u64 + remaining % 10u64));
        remaining /= 10u64;
    }
    return end;
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


ParsedNumber unsigned_number<const Radix:u32 in {10,16}>(CliArgument argument, u64 begin, u64 end, u64 maximum) on cpu {
    if (begin == end) { return ParsedNumber(0u64, NumberError::Empty); }
    u64 index = begin;
    if (argument_byte(argument, index) == 43) { index += 1u64; }
    if (index == end) { return ParsedNumber(0u64, NumberError::Invalid); }
    u64 value = 0u64;
    while (index < end) {
        u8 byte = argument_byte(argument, index);
        u64 digit;
        if (byte >= 48 && byte <= 57) {
            digit = u64(byte) - 48u64;
        } else {
            u8 lower = byte | u8(32);
            if (Radix != 16 || lower < 97 || lower > 102) { return ParsedNumber(0u64, NumberError::Invalid); }
            digit = u64(lower) - 87u64;
        }
        if (value > (maximum - digit) / u64(Radix)) { return ParsedNumber(0u64, NumberError::Overflow); }
        value = value * u64(Radix) + digit;
        index += 1u64;
    }
    return ParsedNumber(value, NumberError::None);
}

extern "C" ParsedNumber decimal_number(address bytes, u64 length, u64 maximum) on cpu {
    return unsigned_number<10>(CliArgument(bytes, length, 1u64), 0u64, length, maximum);
}

extern "C" ParsedNumber hexadecimal_number(address bytes, u64 length, u64 maximum) on cpu {
    return unsigned_number<16>(CliArgument(bytes, length, 1u64), 0u64, length, maximum);
}

LinkBudget parse_budget(CliArgument argument) on cpu {
    if (argument.text == 0u64) { return LinkBudget(0u64, LinkArgumentError::BudgetText); }
    ParsedNumber number = unsigned_number<10>(argument, 0u64, argument.length, 18446744073709551615u64);
    if (number.error == NumberError::Empty) { return LinkBudget(0u64, LinkArgumentError::EmptyBudget); }
    if (number.error == NumberError::Invalid) { return LinkBudget(0u64, LinkArgumentError::InvalidBudget); }
    if (number.error == NumberError::Overflow) { return LinkBudget(0u64, LinkArgumentError::BudgetOverflow); }
    return LinkBudget(number.value, LinkArgumentError::None);
}

extern "C" LinkCommandLine parse_link_arguments(address arguments, u64 count, address input_indices) on cpu {
    LinkCommandLine result = LinkCommandLine(LinkCommand::Link, LinkArgumentError::None, count, 0u64, 0u64, 0u64);
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
            unsafe_position<u64>(unsafe_address(address_bits(input_indices) + result.input_count * size_of<u64>())) = index;
            result.input_count += 1u64;
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
        ParsedNumber group = unsigned_number<10>(argument, begin, colon, 65535u64);
        if (group.error != NumberError::None) { return compile_number_error(group.error); }
        ParsedNumber number = unsigned_number<10>(argument, colon + 1u64, end, 255u64);
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
            ParsedNumber budget = unsigned_number<10>(argument, 0u64, argument.length, 18446744073709551615u64);
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
            ParsedNumber jobs = unsigned_number<10>(argument, 0u64, argument.length, 18446744073709551615u64);
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
            ParsedNumber maximum = unsigned_number<10>(argument, 0u64, argument.length, 4294967295u64);
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
            ParsedNumber start = unsigned_number<10>(value, 0u64, colon, 18446744073709551615u64);
            if (start.error != NumberError::None) {
                line.error = compile_number_error(start.error);
                return line;
            }
            ParsedNumber end = unsigned_number<10>(value, colon + 1u64, value.length, 18446744073709551615u64);
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
                ParsedNumber section = unsigned_number<10>(value, 0u64, value.length, 18446744073709551615u64);
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
            ParsedNumber stage = unsigned_number<10>(value, 0u64, value.length, 18446744073709551615u64);
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
        ParsedNumber number = hex ? unsigned_number<16>(value, 2u64, value.length, 18446744073709551615u64) :
            unsigned_number<10>(value, 0u64, value.length, 18446744073709551615u64);
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

void msvc_text(inout MsvcWriter writer, address text, MsvcText span) on cpu {
    u64 index = 0u64;
    while (index < span.units) {
        address unit = unsafe_address(address_bits(text) + (span.start + index) * 2u64);
        msvc_unit(writer, unsafe_position<u16>(unit));
        index += 1u64;
    }
    msvc_unit(writer, u16(0));
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
    msvc_text(writer, input.text, input.output);
    u64 index = 0u64;
    while (index < input.inputs.count) {
        msvc_text(writer, input.text, msvc_span(input.inputs, index));
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
        msvc_text(writer, input.text, input.import_library);
    }
    index = 0u64;
    while (index < input.search_paths.count) {
        msvc_ascii(writer, "/LIBPATH:");
        msvc_text(writer, input.text, msvc_span(input.search_paths, index));
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
