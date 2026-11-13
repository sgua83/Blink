module bootstrap.link_cli;

struct LinkBudget {
    u64 bytes;
    LinkArgumentError error;
};

LinkArgument argument_at(address arguments, u64 index) on cpu {
    return unsafe_position<LinkArgument>(unsafe_address(address_bits(arguments) + index * size_of<LinkArgument>()));
}

u8 argument_byte(LinkArgument argument, u64 index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(argument.bytes) + index));
}

bool is_token<const N:u32 in {2,6,9,14}>(LinkArgument argument, read array<u8,N> token) on cpu {
    if (argument.length != u64(N)) { return false; }
    u32 index = 0;
    while (index < N) {
        if (argument_byte(argument, u64(index)) != token[index]) { return false; }
        index += 1;
    }
    return true;
}

LinkCommand product(LinkArgument argument) on cpu {
    if (is_token(argument, "--archive")) { return LinkCommand::Archive; }
    if (is_token(argument, "--host-archive")) { return LinkCommand::HostArchive; }
    if (is_token(argument, "--stack-report")) { return LinkCommand::StackReport; }
    return LinkCommand::Link;
}

LinkBudget parse_budget(LinkArgument argument) on cpu {
    if (argument.text == 0u64) { return LinkBudget(0u64, LinkArgumentError::BudgetText); }
    if (argument.length == 0u64) { return LinkBudget(0u64, LinkArgumentError::EmptyBudget); }
    u64 index = 0u64;
    if (argument_byte(argument, index) == 43) { index += 1u64; }
    if (index == argument.length) { return LinkBudget(0u64, LinkArgumentError::InvalidBudget); }
    u64 bytes = 0u64;
    while (index < argument.length) {
        u8 byte = argument_byte(argument, index);
        if (byte < 48 || byte > 57) { return LinkBudget(0u64, LinkArgumentError::InvalidBudget); }
        u64 digit = u64(byte) - 48u64;
        if (bytes > (18446744073709551615u64 - digit) / 10u64) {
            return LinkBudget(0u64, LinkArgumentError::BudgetOverflow);
        }
        bytes = bytes * 10u64 + digit;
        index += 1u64;
    }
    return LinkBudget(bytes, LinkArgumentError::None);
}

extern "C" LinkCommandLine parse_arguments(address arguments, u64 count, address input_indices) on cpu {
    LinkCommandLine result = LinkCommandLine(LinkCommand::Link, LinkArgumentError::None, count, 0u64, 0u64, 0u64);
    if (count == 0u64) {
        result.command = LinkCommand::Help;
        return result;
    }
    LinkArgument first = argument_at(arguments, 0u64);
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
        LinkArgument argument = argument_at(arguments, index);
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
