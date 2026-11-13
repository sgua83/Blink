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


enum NumberError:u32 { None=0; Empty=1; Invalid=2; Overflow=3; };
struct ParsedNumber { u64 value; NumberError error; };

ParsedNumber unsigned_decimal(CliArgument argument, u64 begin, u64 end, u64 maximum) on cpu {
    if (begin == end) { return ParsedNumber(0u64, NumberError::Empty); }
    u64 index = begin;
    if (argument_byte(argument, index) == 43) { index += 1u64; }
    if (index == end) { return ParsedNumber(0u64, NumberError::Invalid); }
    u64 value = 0u64;
    while (index < end) {
        u8 byte = argument_byte(argument, index);
        if (byte < 48 || byte > 57) { return ParsedNumber(0u64, NumberError::Invalid); }
        u64 digit = u64(byte) - 48u64;
        if (value > (maximum - digit) / 10u64) { return ParsedNumber(0u64, NumberError::Overflow); }
        value = value * 10u64 + digit;
        index += 1u64;
    }
    return ParsedNumber(value, NumberError::None);
}

LinkBudget parse_budget(CliArgument argument) on cpu {
    if (argument.text == 0u64) { return LinkBudget(0u64, LinkArgumentError::BudgetText); }
    ParsedNumber number = unsigned_decimal(argument, 0u64, argument.length, 18446744073709551615u64);
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
struct ParsedCpus { u64 count; CompileError error; };

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

ParsedCpus parse_cpus(CliArgument argument, address cpus) on cpu {
    u64 count = 0u64;
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
        if (colon == argument.length) { return ParsedCpus(0u64, CompileError::CpuSyntax); }
        ParsedNumber group = unsigned_decimal(argument, begin, colon, 65535u64);
        if (group.error != NumberError::None) { return ParsedCpus(0u64, compile_number_error(group.error)); }
        ParsedNumber number = unsigned_decimal(argument, colon + 1u64, end, 255u64);
        if (number.error != NumberError::None) { return ParsedCpus(0u64, compile_number_error(number.error)); }
        unsafe_position<u32>(unsafe_address(address_bits(cpus) + count * size_of<u32>())) =
            u32(group.value * 256u64 + number.value);
        count += 1u64;
        begin = end + 1u64;
    }
    return ParsedCpus(count, CompileError::None);
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

CompileCommandLine parse_object_options(address arguments, u64 count, address roles, CompileCommandLine line) on cpu {
    CompileCommandLine result = line;
    u64 index = line.source_index + 1u64;
    while (index < count) {
        CliArgument option = argument_at(arguments, index);
        if (is_token(option, "--module")) {
            result.as_module = 1u64;
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
            ParsedNumber budget = unsigned_decimal(argument, 0u64, argument.length, 18446744073709551615u64);
            if (budget.error != NumberError::None) { result.error = compile_number_error(budget.error); return result; }
            result.budget = budget.value;
            result.has_budget = 1u64;
        } else if (is_token(option, "--entry") || is_token(option, "--program")) {
            if (argument.text == 0u64) { result.error = CompileError::EntryText; return result; }
            if (is_token(option, "--entry")) { result.entry_index = operand; }
            else { result.program_index = operand; }
        } else if (is_token(option, "--import")) {
            unsafe_position<u8>(unsafe_address(address_bits(roles) + operand)) = u8(1);
        } else if (is_token(option, "--instance")) {
            if (argument.text == 0u64) { result.error = CompileError::InstanceText; return result; }
            unsafe_position<u8>(unsafe_address(address_bits(roles) + operand)) = u8(2);
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

extern "C" CompileCommandLine parse_compile_arguments(address arguments, u64 count, address roles, address cpus) on cpu {
    CompileCommandLine result = CompileCommandLine(
        CompileCommand::Unknown, CompileError::None, CompileError::None, 0,
        0u64, count, count, count, count, count, count, count, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    if (count == 0u64) { result.command = CompileCommand::Help; return result; }
    CliArgument first = argument_at(arguments, 0u64);
    if (count == 1u64 && is_token(first, "--version")) { result.command = CompileCommand::Version; return result; }
    result.command = early_command(first);
    if (result.command != CompileCommand::Unknown) {
        if (result.command == CompileCommand::Inputs) {
            if (count != 2u64) { result.error = CompileError::InputsUsage; }
            else { result.source_index = 1u64; }
        }
        return result;
    }

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
            ParsedNumber jobs = unsigned_decimal(argument, 0u64, argument.length, 18446744073709551615u64);
            if (jobs.error != NumberError::None) { result.prefix_error = compile_number_error(jobs.error); return result; }
            if (jobs.value == 0u64) { result.prefix_error = CompileError::ZeroJobs; return result; }
            result.jobs = jobs.value;
        } else if (option == ResourceOption::Cpus) {
            if (result.cpu_count != 0u64) { result.prefix_error = CompileError::DuplicateCpus; return result; }
            ParsedCpus selected = parse_cpus(argument, cpus);
            if (selected.error != CompileError::None) { result.prefix_error = selected.error; return result; }
            result.cpu_count = selected.count;
        } else {
            if (result.has_parameter_limit != 0u64) { result.prefix_error = CompileError::DuplicateParameterLimit; return result; }
            ParsedNumber maximum = unsigned_decimal(argument, 0u64, argument.length, 4294967295u64);
            if (maximum.error != NumberError::None) { result.prefix_error = compile_number_error(maximum.error); return result; }
            result.max_parameter_bytes = u32(maximum.value);
            result.has_parameter_limit = 1u64;
        }
        index += 2u64;
    }
    if (result.jobs != 0u64 && result.cpu_count != 0u64) { result.prefix_error = CompileError::ResourceConflict; return result; }
    result.command_index = index;
    if (index == count) { result.command = CompileCommand::Help; return result; }
    CliArgument command = argument_at(arguments, index);
    if (is_token(command, "--help")) { result.command = CompileCommand::Help; return result; }
    if (command.text == 0u64) { result.error = CompileError::CommandText; return result; }
    result.command = compiler_command(command);
    if (result.command == CompileCommand::Layout || result.command == CompileCommand::Ir) { return result; }
    if (index + 1u64 == count) { result.command = CompileCommand::Unknown; result.error = CompileError::MissingSource; return result; }
    result.source_index = index + 1u64;
    if (result.command == CompileCommand::Object) { return parse_object_options(arguments, count, roles, result); }
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
