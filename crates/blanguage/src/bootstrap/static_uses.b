module bootstrap.static_uses;
import bootstrap.ast_storage;
import bootstrap.names;

address at(address bytes, u64 offset) on cpu {
    return unsafe_address(address_bits(bytes) + offset);
}

T node<T>(UseContext context, u64 offset) on cpu {
    return unsafe_position<T>(at(context.storage.nodes, offset));
}

u64 member_at(UseContext context, Slice list, u64 index) on cpu {
    return node<u64>(context, list.offset + index * size_of<u64>());
}

u32 binding(UseContext context, Text name, u32 kind) on cpu {
    address bytes = context.storage.source;
    if (name.kind != 0) { bytes = context.storage.text; }
    NameBytes text = NameBytes(at(bytes, name.range.start), name.range.end - name.range.start);
    u64 index = 0u64;
    while (index < context.count) {
        BoundName entry = unsafe_position<BoundName>(at(context.bindings, index * size_of<BoundName>()));
        if (entry.kind == kind && compare_name(text, NameBytes(entry.bytes, entry.length)) == i32(0)) {
            return 1 + entry.wrapped;
        }
        index += 1u64;
    }
    return 0;
}

result<void,UseError> argument_uses(UseContext context, Slice list, Span span) on cpu {
    u64 index = 0u64;
    while (index < list.count) {
        ArgumentNode argument = node<ArgumentNode>(context, member_at(context, list, index));
        match (argument) {
            ArgumentNode::Type(ty) { try(type_uses(context, ty, span)); }
            ArgumentNode::Value(shape) { try(shape_uses(context, shape)); }
        }
        index += 1u64;
    }
    return Ok();
}

result<void,UseError> shape_uses(UseContext context, ShapeRef shape) on cpu {
    ShapeNode value = node<ShapeNode>(context, shape.offset);
    match (value) {
        ShapeNode::Literal(unused) { }
        ShapeNode::Constant(unused) { }
        ShapeNode::Expression(expression) { try(expression_uses(context, expression)); }
    }
    return Ok();
}

result<void,UseError> type_uses(UseContext context, TypeRef reference, Span span) on cpu {
    TypeNode ty = node<TypeNode>(context, reference.offset);
    try(argument_uses(context, ty.arguments, span));
    u32 bound = binding(context, ty.name, 1);
    if (bound != 0 && ty.arguments.count != 0u64) { return Err(UseError(1, span)); }
    if (bound == 2 && (ty.dataset != 0 || ty.view != 0 || ty.tile != 0)) {
        return Err(UseError(2, span));
    }
    if (ty.count.present != 0) { try(shape_uses(context, ty.count.value)); }
    return Ok();
}

result<void,UseError> expression_list(UseContext context, Slice expressions) on cpu {
    u64 index = 0u64;
    while (index < expressions.count) {
        try(expression_uses(context, ExprRef(member_at(context, expressions, index))));
        index += 1u64;
    }
    return Ok();
}

result<void,UseError> call_name(UseContext context, Text name, Span span) on cpu {
    if (binding(context, name, 2) != 0) { return Err(UseError(4, span)); }
    return Ok();
}

result<void,UseError> reference_uses(UseContext context, Text name, Slice arguments, Span span) on cpu {
    if (binding(context, name, 1) != 0) { return Err(UseError(3, span)); }
    try(call_name(context, name, span));
    return argument_uses(context, arguments, span);
}

result<void,UseError> expression_uses(UseContext context, ExprRef reference) on cpu {
    ExprNode expression = node<ExprNode>(context, reference.offset);
    match (expression.kind) {
        ExprKind::Number(unused) { }
        ExprKind::StaticU32(unused) { }
        ExprKind::Bool(unused) { }
        ExprKind::Name(unused) { }
        ExprKind::Member(value) { try(expression_uses(context, value.base)); }
        ExprKind::Index(value) {
            try(expression_uses(context, value.base));
            try(expression_uses(context, value.index));
        }
        ExprKind::Call(value) {
            try(call_name(context, value.name, expression.span));
            try(expression_list(context, value.arguments));
        }
        ExprKind::Invoke(value) {
            try(expression_uses(context, value.callee));
            try(expression_list(context, value.arguments));
        }
        ExprKind::Construct(value) {
            try(type_uses(context, value.ty, expression.span));
            try(expression_list(context, value.arguments));
        }
        ExprKind::Specialize(value) {
            try(reference_uses(context, value.name, value.types, expression.span));
            try(expression_list(context, value.arguments));
        }
        ExprKind::FunctionRef(value) {
            try(reference_uses(context, value.name, value.types, expression.span));
        }
        ExprKind::Cooperate(value) {
            if (value.types.present != 0) {
                try(reference_uses(context, value.name, value.types.value, expression.span));
            } else {
                try(call_name(context, value.name, expression.span));
            }
            try(expression_list(context, value.arguments));
        }
        ExprKind::Target(value) { try(expression_uses(context, value.value)); }
        ExprKind::Unary(value) { try(expression_uses(context, value.value)); }
        ExprKind::Binary(value) {
            try(expression_uses(context, value.left));
            try(expression_uses(context, value.right));
        }
        ExprKind::Select(value) {
            try(expression_uses(context, value.condition));
            try(expression_uses(context, value.yes));
            try(expression_uses(context, value.no));
        }
    }
    return Ok();
}

result<void,UseError> optional_expression(UseContext context, Optional<ExprRef> value) on cpu {
    if (value.present != 0) { return expression_uses(context, value.value); }
    return Ok();
}

result<void,UseError> local_name(UseContext context, Text name, Span span) on cpu {
    if (binding(context, name, 2) != 0) { return Err(UseError(5, span)); }
    return Ok();
}

result<void,UseError> statement_uses(UseContext context, StmtNode statement) on cpu {
    match (statement.kind) {
        StmtKind::Declare(value) {
            try(local_name(context, value.name, statement.span));
            try(type_uses(context, value.ty, statement.span));
        }
        StmtKind::Let(value) {
            try(local_name(context, value.name, statement.span));
            if (value.annotation.present != 0) {
                try(type_uses(context, value.annotation.value, statement.span));
            }
            try(expression_uses(context, value.value));
        }
        StmtKind::Assign(value) {
            try(expression_uses(context, value.target));
            try(expression_uses(context, value.value));
        }
        StmtKind::Expr(value) { try(expression_uses(context, value)); }
        StmtKind::If(value) {
            try(expression_uses(context, value.condition));
            try(block_uses(context, value.yes));
            try(block_uses(context, value.no));
        }
        StmtKind::Match(value) {
            try(expression_uses(context, value.value));
            u64 index = 0u64;
            while (index < value.arms.count) {
                ArmNode arm = node<ArmNode>(context, member_at(context, value.arms, index));
                try(block_uses(context, arm.body));
                index += 1u64;
            }
        }
        StmtKind::While(value) {
            try(expression_uses(context, value.condition));
            try(block_uses(context, value.body));
        }
        StmtKind::For(value) {
            try(local_name(context, value.name, statement.span));
            try(expression_uses(context, value.start));
            try(optional_expression(context, value.end));
            try(block_uses(context, value.body));
        }
        StmtKind::Parallel(value) {
            try(local_name(context, value.name, statement.span));
            ParallelNode domain = node<ParallelNode>(context, value.domain.offset);
            try(optional_expression(context, domain.count));
            try(optional_expression(context, domain.selection));
            try(block_uses(context, value.body));
        }
        StmtKind::Return(value) { try(optional_expression(context, value)); }
        StmtKind::Break() { }
        StmtKind::Continue() { }
        StmtKind::Phase() { }
        StmtKind::Seek(value) { try(expression_uses(context, value.value)); }
    }
    return Ok();
}

result<void,UseError> block_uses(UseContext context, Slice body) on cpu {
    u64 index = 0u64;
    while (index < body.count) {
        try(statement_uses(context, node<StmtNode>(context, member_at(context, body, index))));
        index += 1u64;
    }
    return Ok();
}

result<void,UseError> function_uses(UseContext context, FunctionRef reference) on cpu {
    FunctionNode function = node<FunctionNode>(context, reference.offset);
    try(type_uses(context, function.result, function.span));
    u64 index = 0u64;
    while (index < function.params.count) {
        ParamNode parameter = node<ParamNode>(context, member_at(context, function.params, index));
        try(type_uses(context, parameter.ty, parameter.span));
        index += 1u64;
    }
    if (function.body.present != 0) { try(block_uses(context, function.body.value)); }
    return Ok();
}

UseError finish(result<void,UseError> checked) on cpu {
    match (checked) {
        Ok() { return UseError(0, Span(0u64, 0u64)); }
        Err(error) { return error; }
    }
}

extern "C" UseError check_function(UseContext context, FunctionRef function) on cpu {
    return finish(function_uses(context, function));
}

extern "C" UseError check_type(UseContext context, TypeRef ty, Span span) on cpu {
    return finish(type_uses(context, ty, span));
}

extern "C" UseError check_expression(UseContext context, ExprRef expression) on cpu {
    return finish(expression_uses(context, expression));
}
