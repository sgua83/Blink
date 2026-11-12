module bootstrap.participation;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.names;

struct ScanContext { StorageView storage; u64 caller; u32 target; address world; };

address at(address base, u64 offset) on cpu { return unsafe_address(address_bits(base) + offset); }
T source_node<T>(ScanContext context, u64 offset) on cpu {
    return unsafe_position<T>(at(context.storage.nodes, offset));
}
u64 member(ScanContext context, Slice list, u64 index) on cpu {
    return source_node<u64>(context, list.offset + index * size_of<u64>());
}
NameBytes source_name(ScanContext context, Text text) on cpu {
    address base = context.storage.source;
    if (text.kind != 0) { base = context.storage.text; }
    return NameBytes(at(base, text.range.start), text.range.end - text.range.start);
}
bool named<const N:u32>(NameBytes name, read array<u8,N> text) on cpu {
    if (name.length != u64(N)) { return false; }
    for (index in 0..N) {
        if (unsafe_position<u8>(at(name.bytes, u64(index))) != text[index]) { return false; }
    }
    return true;
}
bool task_call(NameBytes name) on cpu {
    return named(name, array<u8,10>(u8(115),u8(116),u8(97),u8(114),u8(116),u8(95),u8(116),u8(97),u8(115),u8(107)))
    || named(name, array<u8,9>(u8(119),u8(97),u8(105),u8(116),u8(95),u8(116),u8(97),u8(115),u8(107)));
}
bool resource_call(NameBytes name) on cpu {
    return named(name, array<u8,7>(u8(100),u8(97),u8(116),u8(97),u8(115),u8(101),u8(116)))
    || named(name, array<u8,6>(u8(117),u8(112),u8(108),u8(111),u8(97),u8(100)))
    || named(name, array<u8,8>(u8(100),u8(111),u8(119),u8(110),u8(108),u8(111),u8(97),u8(100)));
}

bool visit_call(ScanContext context, Text text, bool statement, inout BodyState state,
cfn<u32,address,u64,NameBytes,u32> lookup) on cpu requires nonzero(lookup) {
    NameBytes name = source_name(context, text);
    u32 kind = lookup(context.world, context.caller, name, u32(statement));
    if (kind == 2 && resource_call(name)) { state.host = 1; }
    else if (kind == 4) { state.domain = 1; return true; }
    else if (kind != 0) { state.local |= u32(statement || context.target == 1); }
    return false;
}

void visit_arguments(ScanContext context, Slice arguments, inout BodyState state,
cfn<u32,address,u64,NameBytes,u32> lookup) on cpu requires nonzero(lookup) {
    u64 argument = 0u64;
    while (argument < arguments.count) {
        visit_expression(context, ExprRef(member(context, arguments, argument)), false, state, lookup);
        argument += 1u64;
    }
}

void visit_expression(ScanContext context, ExprRef reference, bool statement, inout BodyState state,
cfn<u32,address,u64,NameBytes,u32> lookup) on cpu requires nonzero(lookup) {
    ExprNode expression = source_node<ExprNode>(context, reference.offset);
    match (expression.kind) {
        ExprKind::Target(value) {
            state.host = 1;
            ExprNode call = source_node<ExprNode>(context, value.value.offset);
            match (call.kind) {
                ExprKind::Call(target_call) { visit_arguments(context, target_call.arguments, state, lookup); }
                ExprKind::Specialize(target_specialized) { visit_arguments(context, target_specialized.arguments, state, lookup); }
                ExprKind::Number(unused) { }
                ExprKind::StaticU32(unused) { }
                ExprKind::Bool(unused) { }
                ExprKind::Name(unused) { }
                ExprKind::Member(unused) { }
                ExprKind::Index(unused) { }
                ExprKind::Invoke(unused) { }
                ExprKind::Construct(unused) { }
                ExprKind::FunctionRef(unused) { }
                ExprKind::Cooperate(unused) { }
                ExprKind::Target(unused) { }
                ExprKind::Unary(unused) { }
                ExprKind::Binary(unused) { }
                ExprKind::Select(unused) { }
            }
        }
        ExprKind::Call(value) {
            if (task_call(source_name(context, value.name))) {
                state.host = 1; visit_arguments(context, value.arguments, state, lookup);
            } else if (!visit_call(context, value.name, statement, state, lookup)) {
                visit_arguments(context, value.arguments, state, lookup);
            }
        }
        ExprKind::Specialize(value) {
            if (!visit_call(context, value.name, statement, state, lookup)) {
                visit_arguments(context, value.arguments, state, lookup);
            }
        }
        ExprKind::Invoke(value) {
            state.local |= u32(statement || context.target == 1);
            visit_expression(context, value.callee, false, state, lookup);
            visit_arguments(context, value.arguments, state, lookup);
        }
        ExprKind::Construct(value) {
            state.local |= u32(context.target == 1); visit_arguments(context, value.arguments, state, lookup);
        }
        ExprKind::Cooperate(value) {
            state.local |= u32(context.target == 1); visit_arguments(context, value.arguments, state, lookup);
        }
        ExprKind::Member(value) {
            state.local |= u32(context.target == 1); visit_expression(context, value.base, false, state, lookup);
        }
        ExprKind::Unary(value) {
            state.local |= u32(context.target == 1); visit_expression(context, value.value, false, state, lookup);
        }
        ExprKind::Index(value) {
            state.local |= u32(context.target == 1);
            visit_expression(context, value.base, false, state, lookup);
            visit_expression(context, value.index, false, state, lookup);
        }
        ExprKind::Binary(value) {
            state.local |= u32(context.target == 1);
            visit_expression(context, value.left, false, state, lookup);
            visit_expression(context, value.right, false, state, lookup);
        }
        ExprKind::Select(value) {
            state.local |= u32(context.target == 1);
            visit_expression(context, value.condition, false, state, lookup);
            visit_expression(context, value.yes, false, state, lookup);
            visit_expression(context, value.no, false, state, lookup);
        }
        ExprKind::Number(unused) { }
        ExprKind::StaticU32(unused) { }
        ExprKind::Bool(unused) { }
        ExprKind::Name(unused) { }
        ExprKind::FunctionRef(unused) { }
    }
}

void visit_block(ScanContext context, Slice block, inout BodyState state,
cfn<u32,address,u64,NameBytes,u32> lookup) on cpu requires nonzero(lookup) {
    u64 statement_index = 0u64;
    while (statement_index < block.count) {
        StmtNode statement = source_node<StmtNode>(context, member(context, block, statement_index));
        match (statement.kind) {
            StmtKind::Parallel(unused) { state.domain = 1; }
            StmtKind::Phase() { state.domain = 1; }
            StmtKind::Expr(value) { visit_expression(context, value, true, state, lookup); }
            StmtKind::For(value) {
                state.local |= u32(context.target == 1);
                visit_expression(context, value.start, false, state, lookup);
                if (value.end.present != 0) { visit_expression(context, value.end.value, false, state, lookup); }
                visit_block(context, value.body, state, lookup);
            }
            StmtKind::If(value) {
                state.local |= u32(context.target == 1);
                visit_expression(context, value.condition, false, state, lookup);
                visit_block(context, value.yes, state, lookup);
                visit_block(context, value.no, state, lookup);
            }
            StmtKind::Match(value) {
                state.local = 1; visit_expression(context, value.value, false, state, lookup);
                u64 arm_index = 0u64;
                while (arm_index < value.arms.count) {
                    ArmNode branch = source_node<ArmNode>(context, member(context, value.arms, arm_index));
                    visit_block(context, branch.body, state, lookup);
                    arm_index += 1u64;
                }
            }
            StmtKind::While(value) {
                state.local = 1; visit_expression(context, value.condition, false, state, lookup);
                visit_block(context, value.body, state, lookup);
            }
            StmtKind::Let(value) {
                state.local |= u32(context.target == 1); visit_expression(context, value.value, false, state, lookup);
            }
            StmtKind::Assign(value) {
                state.local = 1; visit_expression(context, value.target, false, state, lookup);
                visit_expression(context, value.value, false, state, lookup);
            }
            StmtKind::Return(value) {
                if (value.present != 0) { state.local = 1; visit_expression(context, value.value, false, state, lookup); }
            }
            StmtKind::Seek(value) { state.local = 1; visit_expression(context, value.value, false, state, lookup); }
            StmtKind::Declare(unused) { state.local = 1; }
            StmtKind::Break() { state.local = 1; }
            StmtKind::Continue() { state.local = 1; }
        }
        statement_index += 1u64;
    }
}

extern "C" void scan(address functions, u64 count, address states, address context,
cfn<u32,address,u64,NameBytes,u32> lookup) on cpu requires nonzero(lookup) {
    u64 function_index = 0u64;
    while (function_index < count) {
        FunctionInput input = unsafe_position<FunctionInput>(at(functions, function_index * size_of<FunctionInput>()));
        FunctionNode function = unsafe_position<FunctionNode>(at(input.source.storage.nodes, input.source.offset));
        BodyState state = BodyState(0,0,0,0);
        if (input.compiled != 0) {
            state.domain = u32(input.requirement == 1); state.host = u32(input.requirement == 2);
        }
        if (function.kind == 0 && function.body.present != 0) {
            visit_block(ScanContext(input.source.storage, function_index, function.target, context), function.body.value, state, lookup);
        }
        unsafe_position<BodyState>(at(states, function_index * size_of<BodyState>())) = state;
        function_index += 1u64;
    }
}

extern "C" bool less_call(address left, address right, address context) on cpu {
    CallEdge a = unsafe_position<CallEdge>(left); CallEdge b = unsafe_position<CallEdge>(right);
    if (a.caller != b.caller) { return a.caller < b.caller; }
    if (a.callee != b.callee) { return a.callee < b.callee; }
    return a.statement < b.statement;
}

extern "C" void solve(address functions, u64 count, address states, address calls, u64 call_count,
cfn<bool,address,address,address> less) on cpu requires nonzero(less) {
    sort_rows<array<u64,3>>(calls, call_count, unsafe_address(0u64), less);
    u64 edges = 0u64;
    u64 input_edge = 0u64;
    while (input_edge < call_count) {
        CallEdge edge = unsafe_position<CallEdge>(at(calls, input_edge * size_of<CallEdge>()));
        bool duplicate = false;
        if (edges != 0u64) {
            CallEdge prior = unsafe_position<CallEdge>(at(calls, (edges-1u64) * size_of<CallEdge>()));
            if (prior.caller == edge.caller && prior.callee == edge.callee) {
                prior.statement |= edge.statement;
                unsafe_position<CallEdge>(at(calls, (edges-1u64) * size_of<CallEdge>())) = prior;
                duplicate = true;
            }
        }
        if (!duplicate) {
            unsafe_position<CallEdge>(at(calls, edges * size_of<CallEdge>())) = edge; edges += 1u64;
        }
        input_edge += 1u64;
    }
    bool changed = true;
    while (changed) {
        changed = false;
        u64 domain_edge = 0u64;
        while (domain_edge < edges) {
            CallEdge edge = unsafe_position<CallEdge>(at(calls, domain_edge * size_of<CallEdge>()));
            BodyState caller = unsafe_position<BodyState>(at(states, edge.caller * size_of<BodyState>()));
            BodyState callee = unsafe_position<BodyState>(at(states, edge.callee * size_of<BodyState>()));
            if (caller.domain == 0 && callee.domain != 0) { caller.domain = 1; unsafe_position<BodyState>(at(states, edge.caller * size_of<BodyState>())) = caller; changed = true; }
            domain_edge += 1u64;
        }
    }
    u64 body_index = 0u64;
    while (body_index < count) {
        BodyState state = unsafe_position<BodyState>(at(states, body_index * size_of<BodyState>()));
        if (state.domain != 0 && state.local != 0) { state.host = 1; }
        unsafe_position<BodyState>(at(states, body_index * size_of<BodyState>())) = state;
        body_index += 1u64;
    }
    u64 host_edge = 0u64;
    while (host_edge < edges) {
        CallEdge edge = unsafe_position<CallEdge>(at(calls, host_edge * size_of<CallEdge>()));
        BodyState caller = unsafe_position<BodyState>(at(states, edge.caller * size_of<BodyState>()));
        BodyState callee = unsafe_position<BodyState>(at(states, edge.callee * size_of<BodyState>()));
        if (caller.domain != 0 && callee.domain == 0) {
            FunctionInput a = unsafe_position<FunctionInput>(at(functions, edge.caller * size_of<FunctionInput>()));
            FunctionInput b = unsafe_position<FunctionInput>(at(functions, edge.callee * size_of<FunctionInput>()));
            FunctionNode left = unsafe_position<FunctionNode>(at(a.source.storage.nodes, a.source.offset));
            FunctionNode right = unsafe_position<FunctionNode>(at(b.source.storage.nodes, b.source.offset));
            if (edge.statement != 0 || left.target == 1 || right.target == 1) {
                caller.host = 1; unsafe_position<BodyState>(at(states, edge.caller * size_of<BodyState>())) = caller;
            }
        }
        host_edge += 1u64;
    }
    changed = true;
    while (changed) {
        changed = false;
        u64 propagation_edge = 0u64;
        while (propagation_edge < edges) {
            CallEdge edge = unsafe_position<CallEdge>(at(calls, propagation_edge * size_of<CallEdge>()));
            BodyState caller = unsafe_position<BodyState>(at(states, edge.caller * size_of<BodyState>()));
            BodyState callee = unsafe_position<BodyState>(at(states, edge.callee * size_of<BodyState>()));
            if (caller.host == 0 && callee.host != 0) { caller.host = 1; unsafe_position<BodyState>(at(states, edge.caller * size_of<BodyState>())) = caller; changed = true; }
            propagation_edge += 1u64;
        }
    }
    u64 result_index = 0u64;
    while (result_index < count) {
        BodyState state = unsafe_position<BodyState>(at(states, result_index * size_of<BodyState>()));
        state.requirement = 0;
        if (state.host != 0) { state.requirement = 2; }
        else if (state.domain != 0) { state.requirement = 1; }
        unsafe_position<BodyState>(at(states, result_index * size_of<BodyState>())) = state;
        result_index += 1u64;
    }
}
