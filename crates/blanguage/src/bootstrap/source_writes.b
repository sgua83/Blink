module bootstrap.source_writes;

address at(address base, u64 offset) on cpu {
    return unsafe_address(address_bits(base) + offset);
}
T node<T>(StorageView storage, u64 offset) on cpu {
    return unsafe_position<T>(at(storage.nodes, offset));
}
u64 member(StorageView storage, Slice list, u64 index) on cpu {
    return node<u64>(storage, list.offset + index * 8u64);
}
bool spelling<const N:u32>(StorageView storage, bootstrap.ast_storage::Text text, read array<u8,N> expected) on cpu {
    if (text.range.end - text.range.start != u64(N)) { return false; }
    address base = text.kind == 0 ? storage.source : storage.text;
    for (i in 0..N) {
        if (unsafe_position<u8>(at(base, text.range.start + u64(i))) != expected[i]) { return false; }
    }
    return true;
}
ExpressionStep action(u32 phase, ExpressionAction kind) on cpu {
    return ExpressionStep(phase, kind, 0u64, ExprRef(0u64), Access::Value, WritePath::Direct,
        Capture::Discard, bootstrap.ast_storage::Text(0, Span(0u64, 0u64)));
}
ExpressionStep visit(u32 phase, ExprRef child, Access access, WritePath path) on cpu {
    ExpressionStep step = action(phase, ExpressionAction::Visit);
    step.child = child; step.access = access; step.path = path;
    return step;
}
ExpressionStep control(ExprRef child) on cpu {
    ExpressionStep step = visit(1, child, Access::Value, WritePath::Direct);
    step.capture = Capture::Control;
    return step;
}
ExpressionStep resource_step(u32 phase, Access access, u32 resource) on cpu {
    if (access == Access::Value) {
        if (phase == 4) { return action(5, ExpressionAction::Resource); }
        if (phase == 5 && resource != 0) { return action(6, ExpressionAction::Move); }
    }
    return action(6, ExpressionAction::Finish);
}
extern "C" ExpressionStep expression_step(SignatureNode source, u32 phase, u64 argument,
    Access access, u32 current, u32 loop_scope, Truth truth, u32 fact) on cpu {
    StorageView storage = source.storage;
    ExprNode expression = node<ExprNode>(storage, source.offset);
    match (expression.kind) {
        ExprKind::Bool(value) {
            if (phase == 0) {
                ExpressionStep result = action(1, ExpressionAction::Bool);
                result.child.offset = u64(value);
                return result;
            }
        }
        ExprKind::Name(name) {
            if (phase == 0 && loop_scope != 0) { return action(4, ExpressionAction::Name); }
            if (phase == 0 && current != 0) {
                ExpressionStep result = action(1, ExpressionAction::NameEligible);
                result.name = name;
                return result;
            }
            if (phase == 1 && fact != 0) { return action(4, ExpressionAction::Name); }
            return resource_step(phase < 4 ? 4 : phase, access, fact);
        }
        ExprKind::Member(value) {
            if (phase == 0) { return visit(4, value.base, Access::Read, WritePath::Direct); }
            return resource_step(phase, access, fact);
        }
        ExprKind::Index(value) {
            if (phase == 0) { return visit(1, value.base, Access::Read, WritePath::Direct); }
            if (phase == 1) { return visit(4, value.index, Access::Value, WritePath::Direct); }
            return resource_step(phase, access, fact);
        }
        ExprKind::Target(value) {
            if (phase == 0) { return visit(1, value.value, access, WritePath::Direct); }
        }
        ExprKind::Unary(value) {
            if (phase == 0) { return control(value.value); }
            if (phase == 1 && spelling(storage, value.operation, "!")) { return action(2, ExpressionAction::Negate); }
        }
        ExprKind::Binary(value) {
            if (phase == 0) { return control(value.left); }
            bool and = spelling(storage, value.operation, "&&");
            bool or = spelling(storage, value.operation, "||");
            if (phase == 1 && (current == 0 || !((and && truth == Truth::False) || (or && truth == Truth::True)))) {
                ExpressionStep step = visit(2, value.right, Access::Value, and ? WritePath::TrueMoves : or ? WritePath::FalseMoves : WritePath::Direct);
                if (and || or) { step.capture = Capture::Right; }
                return step;
            }
            if (phase < 3) {
                if (and || or) { return action(3, and ? ExpressionAction::And : ExpressionAction::Or); }
                if (current != 0 && (spelling(storage, value.operation, "==") || spelling(storage, value.operation, "!=")
                    || spelling(storage, value.operation, "<") || spelling(storage, value.operation, "<=")
                    || spelling(storage, value.operation, ">") || spelling(storage, value.operation, ">="))) {
                    return action(3, ExpressionAction::Comparison);
                }
            }
        }
        ExprKind::Select(value) {
            if (phase == 0) { return control(value.condition); }
            if (phase == 1 && (current == 0 || truth != Truth::False)) { return visit(2, value.yes, access, WritePath::TrueMoves); }
            if (phase < 3 && (current == 0 || truth != Truth::True)) { return visit(3, value.no, access, WritePath::FalseMoves); }
        }
        ExprKind::Construct(value) {
            if (argument < value.arguments.count) {
                ExpressionStep result = visit(1, ExprRef(member(storage, value.arguments, argument)), Access::Value, WritePath::Direct);
                result.next_argument = argument + 1u64;
                return result;
            }
        }
        ExprKind::Invoke(unused) { if (phase == 0) { return action(1, ExpressionAction::Call); } }
        ExprKind::Call(unused) { if (phase == 0) { return action(1, ExpressionAction::Call); } }
        ExprKind::Specialize(unused) { if (phase == 0) { return action(1, ExpressionAction::Call); } }
        ExprKind::Cooperate(unused) { if (phase == 0) { return action(1, ExpressionAction::Call); } }
        ExprKind::Number(unused) {}
        ExprKind::StaticU32(unused) {}
        ExprKind::FunctionRef(unused) {}
    }
    return action(0, ExpressionAction::Finish);
}

extern "C" StatementStep statement_step(StorageView storage, Slice body, StatementCursor cursor,
    u32 current, Truth truth, u32 present) on cpu {
    StatementStep step = StatementStep(cursor, StatementAction::Finish, WritePath::Direct, Access::Value,
        ExprRef(0u64), Optional<ExprRef>(0, ExprRef(0u64)), Slice(0u64, 0u64),
        bootstrap.ast_storage::Text(0, Span(0u64, 0u64)), bootstrap.ast_storage::Text(0, Span(0u64, 0u64)), Optional<TypeRef>(0, TypeRef(0u64)), current);
    if (cursor.index >= body.count) { return step; }
    StmtNode statement = node<StmtNode>(storage, member(storage, body, step.cursor.index));
    u32 phase = cursor.phase;
    step.cursor.phase += 1;
    match (statement.kind) {
        StmtKind::Declare(value) {
            if (phase == 0) {
                step.action = StatementAction::Declare; step.name = value.name;
                step.annotation = Optional<TypeRef>(1, value.ty);
                return step;
            }
        }
        StmtKind::Let(value) {
            step.value = value.value;
            if (phase == 0) { step.action = StatementAction::Expression; return step; }
            if (phase == 1) {
                step.action = StatementAction::Let; step.name = value.name; step.annotation = value.annotation;
                return step;
            }
        }
        StmtKind::Assign(value) {
            step.value = value.target;
            if (phase == 0 && current != 0) { step.action = StatementAction::Element; return step; }
            if (phase == 1 && present != 0) { step.action = StatementAction::MemoryQuery; return step; }
            if (phase < 3) {
                step.cursor.phase = 3;
                step.action = phase == 2 && present != 0 ? StatementAction::Memory : StatementAction::Position;
                return step;
            }
            if (phase == 3) { step.action = StatementAction::Expression; step.access = Access::Write; return step; }
            if (phase == 4) { step.action = StatementAction::Expression; step.value = value.value; return step; }
        }
        StmtKind::Expr(value) { if (phase == 0) { step.action = StatementAction::Expression; step.value = value; return step; } }
        StmtKind::Return(value) { if (phase == 0 && value.present != 0) { step.action = StatementAction::Expression; step.value = value.value; return step; } }
        StmtKind::Seek(value) { if (phase == 0) { step.action = StatementAction::Expression; step.value = value.value; return step; } }
        StmtKind::If(value) {
            if (phase == 0) { step.action = StatementAction::Expression; step.value = value.condition; return step; }
            if (phase == 1 && truth != Truth::False) {
                step.action = StatementAction::Block; step.body = value.yes;
                step.path = truth == Truth::True ? WritePath::Direct : truth == Truth::Absent ? WritePath::Unknown : WritePath::WhenTrue;
                return step;
            }
            if (phase < 3 && truth != Truth::True) {
                step.cursor.phase = 3; step.action = StatementAction::Block; step.body = value.no;
                step.path = truth == Truth::False ? WritePath::Direct : truth == Truth::Absent ? WritePath::Unknown : WritePath::WhenFalse;
                return step;
            }
        }
        StmtKind::Match(value) {
            if (phase == 0) { step.action = StatementAction::Expression; step.value = value.value; return step; }
            if (step.cursor.arm < value.arms.count) {
                ArmNode arm = node<ArmNode>(storage, member(storage, value.arms, step.cursor.arm));
                if (step.cursor.phase == 2 && arm.binding.present != 0) {
                    step.cursor.phase = 3; step.action = StatementAction::BindArm;
                    step.name = arm.binding.value; step.variant = arm.variant; step.value = value.value;
                    return step;
                }
                if (step.cursor.phase < 5) {
                    step.cursor.phase = 4; step.action = StatementAction::Block; step.body = arm.body;
                    step.path = WritePath::Unknown;
                    return step;
                }
                step.cursor.arm += 1u64;
                step.cursor.phase = 1;
                step.action = arm.binding.present != 0 ? StatementAction::Unbind : StatementAction::Advance;
                return step;
            }
        }
        StmtKind::While(value) {
            step.value = value.condition; step.body = value.body;
            if (phase == 0) { step.action = StatementAction::Expression; step.path = WritePath::Loop; return step; }
            if (phase == 1) { step.action = StatementAction::Block; step.path = WritePath::Loop; return step; }
            if (phase == 2 && current != 0) { step.action = StatementAction::WhileBounds; return step; }
            if (phase < 4) { step.cursor.phase = 4; step.action = StatementAction::Drain; return step; }
        }
        StmtKind::For(value) {
            step.value = value.start; step.end = value.end; step.body = value.body; step.name = value.name;
            if (phase == 0) { step.action = StatementAction::Expression; return step; }
            if (phase == 1 && value.end.present != 0) {
                step.action = StatementAction::Expression; step.value = value.end.value; return step;
            }
            if (phase == 2 && current != 0 && value.end.present != 0) { step.action = StatementAction::RangeBounds; return step; }
            if (phase < 4) {
                step.cursor.phase = 4;
                step.action = value.end.present != 0 ? StatementAction::BindRange : StatementAction::BindElement;
                return step;
            }
            if (phase == 4) {
                step.action = StatementAction::Block;
                step.path = value.end.present != 0 ? WritePath::Range : WritePath::Elements;
                return step;
            }
            if (phase == 5) { step.action = StatementAction::Unbind; return step; }
        }
        StmtKind::Parallel(value) {
            ParallelNode domain = node<ParallelNode>(storage, value.domain.offset);
            if (phase == 0 && domain.count.present != 0) {
                step.action = StatementAction::Expression; step.value = domain.count.value; return step;
            }
            if (phase <= 1 && domain.selection.present != 0) {
                step.cursor.phase = 2; step.action = StatementAction::Expression; step.value = domain.selection.value; return step;
            }
            if (phase <= 2) {
                step.cursor.phase = 3; step.action = StatementAction::BindParallel; step.name = value.name;
                step.variant = domain.dataset;
                return step;
            }
            if (phase == 3) {
                step.action = StatementAction::Block; step.body = value.body; step.path = WritePath::Unknown; return step;
            }
            if (phase == 4) { step.action = StatementAction::Unbind; return step; }
        }
        StmtKind::Break() {}
        StmtKind::Continue() {}
        StmtKind::Phase() {}
    }
    step.cursor.index += 1u64;
    step.cursor.arm = 0u64;
    step.cursor.phase = 0;
    step.action = StatementAction::Advance;
    return step;
}

extern "C" CallSyntax call_syntax(SignatureNode source) on cpu {
    bootstrap.ast_storage::Text empty = bootstrap.ast_storage::Text(0, Span(0u64, 0u64));
    ExprRef none = ExprRef(0u64);
    match (node<ExprNode>(source.storage, source.offset).kind) {
        ExprKind::Call(value) { return CallSyntax(CallForm::Named, value.name, value.arguments, none); }
        ExprKind::Specialize(value) { return CallSyntax(CallForm::Specialized, value.name, value.arguments, none); }
        ExprKind::Cooperate(value) { return CallSyntax(CallForm::Cooperative, value.name, value.arguments, none); }
        ExprKind::Invoke(value) { return CallSyntax(CallForm::Indirect, empty, value.arguments, value.callee); }
        ExprKind::Number(unused) {}
        ExprKind::StaticU32(unused) {}
        ExprKind::Bool(unused) {}
        ExprKind::Name(unused) {}
        ExprKind::Member(unused) {}
        ExprKind::Index(unused) {}
        ExprKind::Construct(unused) {}
        ExprKind::FunctionRef(unused) {}
        ExprKind::Target(unused) {}
        ExprKind::Unary(unused) {}
        ExprKind::Binary(unused) {}
        ExprKind::Select(unused) {}
    }
    return CallSyntax(CallForm::Named, empty, Slice(0u64, 0u64), none);
}

bool position_access(Access access) on cpu {
    return access == Access::Write || access == Access::InOut;
}
ParamNode parameter(SignatureNode declaration, Slice parameters, u64 index) on cpu {
    return node<ParamNode>(declaration.storage, member(declaration.storage, parameters, index));
}
Access declared_access(ParamNode value) on cpu {
    if (value.access == 0) { return Access::Value; }
    if (value.access == 1) { return Access::Read; }
    if (value.access == 2) { return Access::Write; }
    if (value.access == 3) { return Access::InOut; }
    return Access::Atomic;
}
extern "C" CallStep call_step(SignatureNode source, CallSyntax call, CallCursor cursor, CallFacts facts) on cpu {
    CallStep step = CallStep(cursor, CallAction::Advance, ExprRef(0u64), call.name, Access::Value, 0u64, 0, 0);
    StorageView storage = source.storage;
    Slice args = call.arguments;
    bool indirect = call.form == CallForm::Indirect;
    Slice parameters = Slice(0u64, 0u64);
    bool external = false;
    if (cursor.signature == 2 || (cursor.phase == 15 && facts.resolution == CallResolution::Unique)) {
        FunctionNode declaration = node<FunctionNode>(facts.declaration.storage, facts.declaration.offset);
        parameters = declaration.params;
        external = declaration.external.present != 0;
    }
    u32 phase = cursor.phase;
    step.cursor.phase += 1;
    if (phase == 0) {
        if (indirect) { step.action = CallAction::Target; step.value = call.callee; step.cursor.phase = 4; return step; }
        if (facts.current == 0 && args.count == 1u64 && spelling(storage, call.name, "address_of")) {
            step.action = CallAction::Position; step.value = ExprRef(member(storage, args, 0u64)); return step;
        }
    } else if (phase == 1) {
        if (call.form == CallForm::Named) { step.action = CallAction::BoundType; return step; }
    } else if (phase == 2) {
        bool value = facts.bound_type != 0 || spelling(storage, call.name, "own") || spelling(storage, call.name, "into_inner")
            || spelling(storage, call.name, "try") || spelling(storage, call.name, "Ok")
            || spelling(storage, call.name, "Err") || spelling(storage, call.name, "array");
        if (value) {
            step.cursor.default_access = Access::Value;
            step.cursor.phase = 32;
        } else {
            step.cursor.phase = 31; step.action = CallAction::BuiltinType; return step;
        }
    } else if (phase == 3) {
        if (facts.typed == 0) {
            step.cursor.phase = 6;
            if (facts.bound_type != 0 || (call.form == CallForm::Specialized && facts.record != 0)) {
                step.cursor.phase = 18; return step;
            }
            step.action = CallAction::Declared; return step;
        }
    } else if (phase == 4) {
        step.cursor.signature = 1; step.action = CallAction::Function; return step;
    } else if (phase == 5) {
        if (facts.function == 0) { step.action = CallAction::BadSignature; return step; }
        step.cursor.phase = 10;
        if (facts.known_memory_write != 0) { step.action = CallAction::External; return step; }
    } else if (phase == 6) {
        if (facts.resolution == CallResolution::Unique) {
            step.cursor.signature = 2;
            if (facts.current != 0) { step.action = CallAction::Instance; return step; }
            // Read the declaration on the next invocation.
            return step;
        }
        step.cursor.phase = facts.resolution == CallResolution::Missing ? 14 : 18;
    } else if (phase == 7) {
        if (external) { step.action = CallAction::MemoryContract; return step; }
    } else if (phase == 8) {
        if (facts.instantiated != 0) {
            u64 i = 0u64;
            while (i < parameters.count && i < args.count) {
                ParamNode param = parameter(facts.declaration, parameters, i);
                if (position_access(declared_access(param)) && node<TypeNode>(facts.declaration.storage, param.ty.offset).tile == 0) {
                    step.cursor.phase = 11; step.action = CallAction::ReadOnly; return step;
                }
                i += 1u64;
            }
        }
        step.cursor.phase = 11;
    } else if (phase == 10) {
        if (step.cursor.index >= args.count || step.cursor.index >= facts.parameter_count) {
            step.cursor.index = 0u64;
            step.cursor.phase = indirect ? 22 : 18;
        } else {
            step.action = CallAction::Parameter; step.parameter = step.cursor.index; return step;
        }
    } else if (phase == 11) {
        if (step.cursor.signature == 1) {
            u64 index = step.cursor.index;
            step.cursor.index += 1u64; step.cursor.phase = 10;
            if (position_access(facts.parameter_access)) {
                step.action = CallAction::Position; step.value = ExprRef(member(storage, args, index));
                step.local = facts.current; return step;
            }
        } else {
            if (step.cursor.index >= args.count || step.cursor.index >= parameters.count) {
                step.cursor.index = 0u64; step.cursor.phase = 18;
                if (!external || facts.known_memory_write != 0) { step.action = CallAction::External; return step; }
            } else {
                ParamNode param = parameter(facts.declaration, parameters, step.cursor.index);
                TypeNode ty = node<TypeNode>(facts.declaration.storage, param.ty.offset);
                if (position_access(declared_access(param)) && ty.tile == 0) {
                    if (ty.dataset == 0) {
                        step.action = CallAction::Contents; step.value = ExprRef(member(storage, args, step.cursor.index));
                        return step;
                    }
                    u64 index = step.cursor.index;
                    step.cursor.index += 1u64; step.cursor.phase = 11;
                    if (facts.read_only == 0) {
                        step.action = CallAction::Position; step.value = ExprRef(member(storage, args, index));
                        step.contents = 1; step.local = facts.current; return step;
                    }
                } else {
                    step.cursor.index += 1u64; step.cursor.phase = 11;
                }
            }
        }
    } else if (phase == 12) {
        u64 index = step.cursor.index;
        step.cursor.index += 1u64; step.cursor.phase = 11;
        if (facts.read_only == 0) {
            step.action = CallAction::Position; step.value = ExprRef(member(storage, args, index));
            step.contents = facts.contents; step.local = facts.current; return step;
        }
    } else if (phase == 14) {
        step.cursor.phase = 16;
        if (facts.current == 0 && args.count >= 2u64 && spelling(storage, call.name, "start_task")) {
            ExprNode worker = node<ExprNode>(storage, member(storage, args, 0u64));
            match (worker.kind) {
                ExprKind::Name(name) { step.worker = name; step.cursor.phase = 15; }
                ExprKind::FunctionRef(value) { if (value.types.count == 0u64) { step.worker = value.name; step.cursor.phase = 15; } }
                ExprKind::Number(unused) {}
                ExprKind::StaticU32(unused) {}
                ExprKind::Bool(unused) {}
                ExprKind::Member(unused) {}
                ExprKind::Index(unused) {}
                ExprKind::Call(unused) {}
                ExprKind::Invoke(unused) {}
                ExprKind::Construct(unused) {}
                ExprKind::Specialize(unused) {}
                ExprKind::Cooperate(unused) {}
                ExprKind::Target(unused) {}
                ExprKind::Unary(unused) {}
                ExprKind::Binary(unused) {}
                ExprKind::Select(unused) {}
            }
            if (step.cursor.phase == 15) { step.action = CallAction::Worker; return step; }
        }
    } else if (phase == 15) {
        if (facts.resolution == CallResolution::Unique) {
            if (step.cursor.parameter < parameters.count && step.cursor.index < args.count - 2u64) {
                ParamNode param = parameter(facts.declaration, parameters, step.cursor.parameter);
                step.cursor.parameter += 1u64;
                step.cursor.phase = 15;
                if (declared_access(param) != Access::Value) {
                    step.action = CallAction::Position;
                    step.value = ExprRef(member(storage, args, step.cursor.index + 2u64));
                    step.cursor.index += 1u64; return step;
                }
                return step;
            }
        }
        step.cursor.index = 0u64;
    } else if (phase == 16) {
        step.cursor.phase = 18;
        if (args.count == 2u64 && spelling(storage, call.name, "take_last")) {
            step.action = CallAction::Position; step.value = ExprRef(member(storage, args, 1u64)); return step;
        }
    } else if (phase == 18) {
        if (call.form == CallForm::Specialized && args.count == 2u64 && spelling(storage, call.name, "dataset")) {
            u64 length = member(storage, args, 0u64);
            match (node<ExprNode>(storage, length).kind) {
                ExprKind::StaticU32(value) { if (value == 0) { step.cursor.phase = 23; } }
                ExprKind::Number(value) { step.action = CallAction::LengthNumber; step.worker = value; step.value = ExprRef(length); return step; }
                ExprKind::Name(value) { step.action = CallAction::LengthName; step.worker = value; step.cursor.phase = 30; return step; }
                ExprKind::Bool(unused) {}
                ExprKind::Member(unused) {}
                ExprKind::Index(unused) {}
                ExprKind::Call(unused) {}
                ExprKind::Invoke(unused) {}
                ExprKind::Construct(unused) {}
                ExprKind::Specialize(unused) {}
                ExprKind::FunctionRef(unused) {}
                ExprKind::Cooperate(unused) {}
                ExprKind::Target(unused) {}
                ExprKind::Unary(unused) {}
                ExprKind::Binary(unused) {}
                ExprKind::Select(unused) {}
            }
        } else {
            step.cursor.phase = 23;
        }
    } else if (phase == 19) {
        if (facts.empty != 0) { step.cursor.phase = 23; }
        else if (facts.current != 0) {
            step.action = CallAction::Factory; step.value = ExprRef(member(storage, args, 1u64)); return step;
        } else {
            step.cursor.phase = 23; step.action = CallAction::Generator;
            step.value = ExprRef(member(storage, args, 1u64)); return step;
        }
    } else if (phase == 20) {
        if (facts.instantiated != 0 && facts.factory_owns == 0) {
            step.action = CallAction::FactoryEffects; return step;
        }
        step.cursor.phase = 23; step.action = CallAction::Generator;
        step.value = ExprRef(member(storage, args, 1u64)); return step;
    } else if (phase == 21) {
        step.cursor.phase = 23;
        if (facts.read_only == 0) {
            step.action = CallAction::Generator; step.value = ExprRef(member(storage, args, 1u64)); return step;
        }
    } else if (phase == 22) {
        step.action = CallAction::Visit; step.value = call.callee; step.access = Access::Read; return step;
    } else if (phase == 23) {
        u64 index = step.cursor.index;
        if (index >= args.count || (indirect && index >= facts.parameter_count)) {
            step.action = CallAction::Finish;
            return step;
        }
        if (step.cursor.signature == 1 && index < facts.parameter_count) {
            step.action = CallAction::Parameter; step.parameter = index; return step;
        }
        step.cursor.index += 1u64; step.cursor.phase = 23;
        step.action = CallAction::Visit; step.value = ExprRef(member(storage, args, index));
        step.access = step.cursor.signature == 2 && index < parameters.count
            ? declared_access(parameter(facts.declaration, parameters, index)) : step.cursor.default_access;
        return step;
    } else if (phase == 24) {
        step.action = CallAction::Visit;
        step.value = ExprRef(member(storage, args, step.cursor.index));
        step.access = facts.parameter_access; step.cursor.index += 1u64; step.cursor.phase = 23;
        return step;
    } else if (phase == 30) {
        step.cursor.phase = 19;
        if (facts.initialized_length != 0) {
            match (node<ExprNode>(facts.length.storage, facts.length.offset).kind) {
                ExprKind::StaticU32(value) { if (value == 0) { step.cursor.phase = 23; } }
                ExprKind::Number(value) { step.action = CallAction::LengthNumber; step.worker = value; return step; }
                ExprKind::Bool(unused) {}
                ExprKind::Name(unused) {}
                ExprKind::Member(unused) {}
                ExprKind::Index(unused) {}
                ExprKind::Call(unused) {}
                ExprKind::Invoke(unused) {}
                ExprKind::Construct(unused) {}
                ExprKind::Specialize(unused) {}
                ExprKind::FunctionRef(unused) {}
                ExprKind::Cooperate(unused) {}
                ExprKind::Target(unused) {}
                ExprKind::Unary(unused) {}
                ExprKind::Binary(unused) {}
                ExprKind::Select(unused) {}
            }
        }
    } else if (phase == 31) {
        step.cursor.default_access = facts.builtin_type != 0 ? Access::Value : Access::Read;
    } else if (phase == 32) {
        // Resolving a record registers its source build dependency. Intrinsics
        // and bound types only query it when the original specialization can use it.
        if (step.cursor.default_access == Access::Read || call.form == CallForm::Specialized) {
            step.action = CallAction::RecordType; return step;
        }
    } else if (phase == 33) {
        step.cursor.phase = 3;
        if (facts.record != 0) { step.cursor.default_access = Access::Value; }
        if (facts.bound_type == 0 && call.form == CallForm::Named) { step.action = CallAction::Local; return step; }
    }
    return step;
}
