module bootstrap.source_writes;

struct LoopSlotCell { OrderedNode node; usize slot; };
struct LoopSlotSearch { usize parent; i32 order; };

LoopSlotSearch find_loop_slot(OrderedTree tree, usize slot) on cpu {
    usize current = tree.root;
    usize parent = 0u64;
    i32 order = i32(1);
    while (current != 0u64) {
        let row = unsafe_address(address_bits(tree.records) + (current - 1u64) * size_of<LoopSlotCell>());
        let cell = unsafe_position<LoopSlotCell>(row);
        if (slot == cell.slot) { return LoopSlotSearch(current, i32(0)); }
        order = slot < cell.slot ? i32(-1) : i32(1);
        parent = current;
        current = order < 0 ? cell.node.left : cell.node.right;
    }
    return LoopSlotSearch(parent, order);
}

extern "C" u32 has_loop_slot(address slots, usize slot) on cpu {
    return u32(find_loop_slot(unsafe_position<OrderedTree>(slots), slot).order == 0);
}

extern "C" void insert_loop_slot(inout OrderedTree slots, usize slot) on cpu {
    let selected = find_loop_slot(slots, slot);
    if (selected.order == 0) { return; }
    let owner = unsafe_address(address_bits(address_of(slots)));
    let cell = ordered_tree_prepare(owner, size_of<LoopSlotCell>());
    unsafe_position<LoopSlotCell>(cell).slot = slot;
    ordered_tree_publish(owner, size_of<LoopSlotCell>(), selected.parent, selected.order);
}

extern "C" LoopProjection loop_projection(LoopWriteKind kind, u32 address_write, u32 iterator) on cpu {
    if (kind == LoopWriteKind::Move) { return LoopProjection::Ignore; }
    if (kind == LoopWriteKind::External || address_write != 0) { return LoopProjection::External; }
    if (kind == LoopWriteKind::Memory) { return LoopProjection::Memory; }
    if (iterator != 0) { return LoopProjection::Unknown; }
    if (kind == LoopWriteKind::Contents) { return LoopProjection::Contents; }
    return LoopProjection::Position;
}

extern "C" address realloc(address data, usize bytes) on cpu from "ucrtbase.dll";
extern "C" void free(address data) on cpu from "ucrtbase.dll";
extern "C" never abort() on cpu from "ucrtbase.dll";

extern "C" void push_loop_storage(inout LoopStorage storage, StorageWrite written) on cpu {
    if (storage.count == storage.capacity) {
        // The same initialized bytes can be borrowed as a Rust slice.
        let limit = 9223372036854775807u64 / size_of<StorageWrite>();
        if (storage.count == limit) { abort(); }
        let next = min(limit, max(8u64, storage.capacity * 2u64));
        let data = realloc(storage.data, next * size_of<StorageWrite>());
        if (data == address()) { abort(); }
        storage.data = data;
        storage.capacity = next;
    }
    let byte = address_bits(storage.data) + storage.count * size_of<StorageWrite>();
    unsafe_position<StorageWrite>(unsafe_address(byte)) = written;
    storage.count += 1u64;
}

extern "C" void drop_loop_storage(inout LoopStorage storage) on cpu {
    free(storage.data);
    storage = LoopStorage(address(), 0u64, 0u64);
}

extern "C" void project_loop_storage(inout LoopStorage output, LoopStorageHost host, usize count, u32 constant_index, u32 index) on cpu
    requires nonzero(host.site) requires nonzero(host.start_parts) requires nonzero(host.next_part) {
    usize location = 0u64;
    while (location < count) {
        let site = host.site(host.context, location);
        if (site.descendants != 0) {
            push_loop_storage(output, StorageWrite(site.root, LoopBytes(0u64, 0u64), StorageWriteKind::Descendants));
        } else if (site.declared == 0 || constant_index == 0) {
            push_loop_storage(output, StorageWrite(site.root, LoopBytes(0u64, 0u64), StorageWriteKind::Frame));
        } else {
            let stride = host.start_parts(host.context, location);
            // All terms are u32. Their product has room for both additions.
            let element = u64(index) * u64(stride);
            while (true) {
                let part = host.next_part(host.context);
                if (part.present == 0) { break; }
                let begin = element + u64(part.word);
                push_loop_storage(output, StorageWrite(site.root, LoopBytes(begin, begin + u64(part.width)), StorageWriteKind::Bytes));
            }
        }
        location += 1u64;
    }
}

extern "C" LoopBytes loop_array_elements(u64 length, u32 exact, u32 index) on cpu {
    if (exact == 0) { return LoopBytes(0u64, length); }
    let element = u64(index);
    if (element < length) { return LoopBytes(element, element + 1u64); }
    return LoopBytes(0u64, 0u64);
}

extern "C" u32 loop_part_matches(u32 source_scalar, u32 source_word, u32 target_scalar, u32 target_word, u32 offset) on cpu {
    return u32(source_scalar == target_scalar && source_word == target_word + offset);
}

extern "C" u32 loop_contents_frame(u32 found, u32 owns_resources) on cpu {
    return u32(found != 0 && owns_resources == 0);
}

extern "C" LoopUncertainty loop_position_uncertainty(LoopPosition position, u32 written, u32 has_binding, u32 direct) on cpu {
    if (has_binding == 0) { return LoopUncertainty(u32(direct == 0), 0); }
    if (position == LoopPosition::Local && written != 0) { return LoopUncertainty(0, 0); }
    return LoopUncertainty(1, u32(position == LoopPosition::Reference));
}

extern "C" LoopUncertainty loop_memory_uncertainty(LoopPosition position, u32 written) on cpu {
    if (written == 0 || position == LoopPosition::Local) { return LoopUncertainty(0, 0); }
    return LoopUncertainty(1, u32(position == LoopPosition::Reference));
}

extern "C" LoopInvalidation loop_invalidation_next(LoopInvalidation phase) on cpu {
    if (phase == LoopInvalidation::Elements) { return LoopInvalidation::Rebind; }
    if (phase == LoopInvalidation::Rebind) { return LoopInvalidation::Select; }
    if (phase == LoopInvalidation::Select) { return LoopInvalidation::Apply; }
    return LoopInvalidation::Done;
}

extern "C" LoopElementEffect loop_element_effect(LoopWriteKind kind, u32 address_write, u32 resolved) on cpu {
    if (kind == LoopWriteKind::Move) { return LoopElementEffect(LoopElementWrite::None, 0, 0); }
    if (kind == LoopWriteKind::Memory) {
        if (resolved != 0) { return LoopElementEffect(LoopElementWrite::Bytes, 1, 0); }
        return LoopElementEffect(LoopElementWrite::Unknown, 1, 1);
    }
    if (kind == LoopWriteKind::External) { return LoopElementEffect(LoopElementWrite::Unknown, 1, 0); }
    if (kind == LoopWriteKind::Contents) { return LoopElementEffect(LoopElementWrite::Contents, 0, 0); }
    return LoopElementEffect(LoopElementWrite::Position, address_write, 0);
}

extern "C" LoopIndexRange loop_index_range(u32 constant, u32 index) on cpu {
    if (constant != 0) { return LoopIndexRange(index, index); }
    return LoopIndexRange(0, 0xffffffff);
}

extern "C" LoopRebind loop_rebind(u32 held, u32 identity, u32 borrowed, u32 checked, u32 written) on cpu {
    if (held != 0 || borrowed != 0) { return LoopRebind::Keep; }
    if (identity != 0 && checked == 0) { return LoopRebind::CheckBorrow; }
    if (checked < 2) { return LoopRebind::Scan; }
    if (written != 0) { return LoopRebind::Forget; }
    return LoopRebind::Keep;
}

extern "C" u32 loop_scalar_header(u32 conditional, u32 owned, u32 unsigned_scalar, LoopPosition position) on cpu {
    return u32(conditional != 0 && owned != 0 && unsigned_scalar != 0
        && (position == LoopPosition::Local || position == LoopPosition::Reference));
}

extern "C" u32 loop_cursor_query(LoopWriteKind kind, u32 cursor, u32 application_element) on cpu {
    return u32(kind == LoopWriteKind::Position && cursor != 0 && application_element != 0);
}

extern "C" LoopComponentWrite loop_component_write(LoopWriteKind kind, u32 cursor, u32 same_resource, u32 same_layout, u32 same_index) on cpu {
    if (kind == LoopWriteKind::Move || (cursor != 0 && same_resource != 0 && same_layout != 0 && same_index != 0)) {
        return LoopComponentWrite::None;
    }
    if (kind == LoopWriteKind::External) { return LoopComponentWrite::All; }
    if (kind == LoopWriteKind::Memory) {
        if (cursor != 0) { return LoopComponentWrite::OutsideElement; }
        return LoopComponentWrite::Memory;
    }
    if (kind == LoopWriteKind::Contents) { return LoopComponentWrite::Contents; }
    return LoopComponentWrite::Position;
}

extern "C" u32 loop_deferred_input(u32 deferred, u32 view_scope) on cpu {
    return u32(deferred != 0 && view_scope != 0);
}

extern "C" u32 loop_metadata_contains(u32 begin, u32 end, u32 word) on cpu {
    return u32(begin <= word && word < end);
}

extern "C" u32 loop_external_proof(u32 raw, u32 view_scope) on cpu {
    return u32(raw != 0 && view_scope == 0);
}

extern "C" u32 loop_local_input(u32 view_scope, u32 entry, u32 direct) on cpu {
    return u32(view_scope != 0 && entry != 0 && direct == 0);
}

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
            if (facts.current == 0 && facts.parameter_access == Access::Read) {
                step.action = CallAction::ReadPlace; step.value = ExprRef(member(storage, args, index)); return step;
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
                    u64 index = step.cursor.index;
                    step.cursor.index += 1u64; step.cursor.phase = 11;
                    if (facts.current == 0 && declared_access(param) == Access::Read && ty.tile == 0) {
                        step.action = CallAction::ReadPlace; step.value = ExprRef(member(storage, args, index)); return step;
                    }
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

IntegerWide wide(u64 value) on cpu { return IntegerWide(value, 0u64); }

NumericTransfer numeric_kind(NumericKind kind, u32 flags) on cpu {
    IntegerWide zero = wide(0u64);
    return NumericTransfer(kind, flags, zero, zero, zero, zero);
}

extern "C" NumericTransfer numeric_shift(u32 amount, u32 subtract) on cpu {
    IntegerWide zero = wide(0u64);
    IntegerWide step = wide(u64(amount));
    if (subtract != 0) {
        step = range_negate(step);
        return NumericTransfer(NumericKind::Shift, 0, step, step, step, zero);
    }
    return NumericTransfer(NumericKind::Shift, 0, step, step, zero, step);
}

bool is_zero(IntegerWide value) on cpu {
    return value.low == 0u64 && value.high == 0u64;
}

extern "C" bool numeric_zero_delta(NumericTransfer effect) on cpu {
    return effect.kind == NumericKind::Shift && is_zero(effect.low) && is_zero(effect.high);
}

extern "C" bool numeric_identity(NumericTransfer effect) on cpu {
    return numeric_zero_delta(effect)
        && is_zero(effect.least) && is_zero(effect.most);
}

NumericTransfer numeric_ordered(u32 flags) on cpu {
    return numeric_kind(flags == 0 ? NumericKind::Unknown : NumericKind::Ordered, flags);
}

extern "C" NumericCombination numeric_then(NumericTransfer left, NumericTransfer right) on cpu {
    NumericTransfer unknown = numeric_kind(NumericKind::Unknown, 0);
    if (right.kind == NumericKind::Assign) { return NumericCombination(right, NumericProofAction::Right); }
    if ((left.kind == NumericKind::Assign || left.kind == NumericKind::Reflect || left.kind == NumericKind::Ordered)
        && numeric_identity(right)) { return NumericCombination(left, NumericProofAction::Left); }
    if (numeric_identity(left) && (right.kind == NumericKind::Ordered || right.kind == NumericKind::Reflect)) {
        return NumericCombination(right, NumericProofAction::Right);
    }
    if (left.kind == NumericKind::Assign && right.kind == NumericKind::Ordered) {
        return NumericCombination(left, NumericProofAction::Retain);
    }
    if (left.kind == NumericKind::Ordered && right.kind == NumericKind::Ordered) {
        return NumericCombination(numeric_ordered(left.flags & right.flags), NumericProofAction::None);
    }
    if (left.kind == NumericKind::Assign && right.kind == NumericKind::Shift) {
        return NumericCombination(left, NumericProofAction::Shift);
    }
    if (left.kind == NumericKind::Reflect && right.kind == NumericKind::Reflect && left.flags == right.flags) {
        return NumericCombination(left, NumericProofAction::None);
    }
    if (left.kind != NumericKind::Shift || right.kind != NumericKind::Shift) {
        return NumericCombination(unknown, NumericProofAction::None);
    }
    IntegerWideChecked low = integer_wide_checked_add(left.low, right.low);
    IntegerWideChecked high = integer_wide_checked_add(left.high, right.high);
    IntegerWideChecked least = integer_wide_checked_add(left.low, right.least);
    IntegerWideChecked most = integer_wide_checked_add(left.high, right.most);
    if (low.present == 0 || high.present == 0 || least.present == 0 || most.present == 0) {
        return NumericCombination(unknown, NumericProofAction::None);
    }
    return NumericCombination(NumericTransfer(NumericKind::Shift, 0, low.value, high.value,
        range_min(left.least, least.value), range_max(left.most, most.value)), NumericProofAction::None);
}

bool numeric_join_identity(NumericTransfer effect) on cpu {
    return effect.kind == NumericKind::Assign || effect.kind == NumericKind::Ordered || effect.kind == NumericKind::Reflect;
}

extern "C" NumericCombination numeric_join(NumericTransfer left, NumericTransfer right) on cpu {
    if (left.kind == NumericKind::Missing) { return NumericCombination(right, NumericProofAction::Right); }
    if (right.kind == NumericKind::Missing) { return NumericCombination(left, NumericProofAction::Left); }
    if (left.kind == NumericKind::Assign && right.kind == NumericKind::Assign) {
        return NumericCombination(left, NumericProofAction::Join);
    }
    if (left.kind == NumericKind::Ordered && right.kind == NumericKind::Ordered) {
        return NumericCombination(numeric_ordered(left.flags & right.flags), NumericProofAction::None);
    }
    if (left.kind == NumericKind::Reflect && right.kind == NumericKind::Reflect && left.flags == right.flags) {
        return NumericCombination(left, NumericProofAction::None);
    }
    if (numeric_identity(right) && numeric_join_identity(left)) { return NumericCombination(left, NumericProofAction::Left); }
    if (numeric_identity(left) && numeric_join_identity(right)) { return NumericCombination(right, NumericProofAction::Right); }
    if (left.kind == NumericKind::Shift && right.kind == NumericKind::Shift) {
        return NumericCombination(NumericTransfer(NumericKind::Shift, 0, range_min(left.low, right.low),
            range_max(left.high, right.high), range_min(left.least, right.least), range_max(left.most, right.most)),
            NumericProofAction::None);
    }
    return NumericCombination(numeric_kind(NumericKind::Unknown, 0), NumericProofAction::None);
}

bool numeric_no_wrap(NumericTransfer effect, IntegerWide lower, IntegerWide upper) on cpu {
    IntegerWideChecked least = integer_wide_checked_add(lower, effect.least);
    IntegerWideChecked most = integer_wide_checked_add(upper, effect.most);
    return least.present != 0 && most.present != 0 && !range_less(least.value, wide(0u64))
        && !range_less(wide(4294967295u64), most.value);
}

u32 numeric_order(bool lower, bool upper) on cpu {
    return (lower ? 1 : 0) | (upper ? 2 : 0);
}

u32 numeric_shift_order(NumericTransfer effect) on cpu {
    return numeric_order(!range_less(effect.low, wide(0u64)), !range_less(wide(0u64), effect.high));
}

extern "C" NumericTransfer numeric_nested_order(NumericTransfer effect, NumericRange guard, NumericRange assigned) on cpu {
    if (effect.kind == NumericKind::Missing) { return numeric_ordered(3); }
    if (effect.kind == NumericKind::Ordered) { return effect; }
    if (effect.kind == NumericKind::Assign) {
        return numeric_ordered(numeric_order(assigned.lower >= guard.upper, assigned.upper <= guard.lower));
    }
    if (effect.kind == NumericKind::Shift && numeric_no_wrap(effect, wide(u64(guard.lower)), wide(u64(guard.upper)))) {
        return numeric_ordered(numeric_shift_order(effect));
    }
    return numeric_kind(NumericKind::Unknown, 0);
}

extern "C" NumericIterations numeric_iterations(NumericTransfer effect, NumericRange input, NumericRange guard) on cpu {
    if (effect.kind != NumericKind::Shift) { return NumericIterations(0, 0u64); }
    if (input.upper < guard.lower || input.lower > guard.upper) { return NumericIterations(1, 0u64); }
    if (!numeric_no_wrap(effect, wide(u64(guard.lower)), wide(u64(guard.upper)))) {
        return NumericIterations(0, 0u64);
    }
    IntegerWide divisor;
    u32 distance;
    if (range_less(effect.high, wide(0u64))) {
        divisor = range_negate(effect.high);
        if (range_less(divisor, wide(0u64))) { return NumericIterations(0, 0u64); }
        distance = input.upper - guard.lower;
    } else if (range_less(wide(0u64), effect.low)) {
        divisor = effect.low;
        distance = guard.upper - input.lower;
    } else {
        return NumericIterations(0, 0u64);
    }
    return NumericIterations(1, integer_wide_unsigned_divide(wide(u64(distance)), divisor).quotient.low + 1u64);
}

IntegerWideChecked numeric_endpoint(u32 input, u64 iterations, IntegerWide shift, bool lower) on cpu {
    IntegerWideChecked delta = integer_wide_checked_multiply(wide(iterations), shift);
    if (delta.present == 0) { return delta; }
    IntegerWide extreme = lower ? range_min(delta.value, wide(0u64)) : range_max(delta.value, wide(0u64));
    return integer_wide_checked_add(wide(u64(input)), extreme);
}

extern "C" NumericRanges numeric_ranges(NumericTransfer effect, NumericRange input, NumericIterations iterations) on cpu {
    NumericRange empty = NumericRange(0, 0);
    NumericRanges missing = NumericRanges(0, empty, empty);
    if (iterations.present == 0 || effect.kind != NumericKind::Shift) { return missing; }
    // The entry remains a sound range for a body that never executes.
    u64 before = iterations.count == 0u64 ? 0u64 : iterations.count - 1u64;
    IntegerWideChecked body_low = numeric_endpoint(input.lower, before, effect.low, true);
    IntegerWideChecked body_high = numeric_endpoint(input.upper, before, effect.high, false);
    if (body_low.present == 0 || body_high.present == 0 || !numeric_no_wrap(effect, body_low.value, body_high.value)) {
        return missing;
    }
    IntegerWideChecked header_low = numeric_endpoint(input.lower, iterations.count, effect.low, true);
    IntegerWideChecked header_high = numeric_endpoint(input.upper, iterations.count, effect.high, false);
    if (header_low.present == 0 || header_high.present == 0) { return missing; }
    return NumericRanges(1, NumericRange(u32(body_low.value.low), u32(body_high.value.low)),
        NumericRange(u32(header_low.value.low), u32(header_high.value.low)));
}

extern "C" NumericRetention numeric_retention(NumericTransfer effect, NumericRange input, NumericRange guard, u32 bounded) on cpu {
    if (effect.kind == NumericKind::Missing) { return NumericRetention(3, NumericProofAction::None); }
    if (effect.kind == NumericKind::Ordered) { return NumericRetention(effect.flags, NumericProofAction::None); }
    if (effect.kind == NumericKind::Assign) { return NumericRetention(3, NumericProofAction::Join); }
    if (effect.kind == NumericKind::Reflect) { return NumericRetention(3, NumericProofAction::Reflect); }
    if (effect.kind == NumericKind::Shift) {
        u32 flags = numeric_shift_order(effect);
        u32 lower = max(guard.lower, (flags & 1) != 0 ? input.lower : 0);
        u32 upper = min(guard.upper, (flags & 2) != 0 ? input.upper : u32(4294967295u64));
        if (bounded != 0 || numeric_no_wrap(effect, wide(u64(lower)), wide(u64(upper)))) {
            return NumericRetention(flags, NumericProofAction::None);
        }
    }
    return NumericRetention(0, NumericProofAction::None);
}

bool numeric_arguments_contain(StorageView storage, Slice arguments, ExprRef selected) on cpu {
    u64 i = 0u64; while (i < arguments.count) {
        if (numeric_expression_contains(storage, ExprRef(member(storage, arguments, i)), selected)) { return true; }
        i += 1u64;
    }
    return false;
}

bool numeric_expression_contains(StorageView storage, ExprRef root, ExprRef selected) on cpu {
    if (root.offset == selected.offset) { return true; }
    match (node<ExprNode>(storage, root.offset).kind) {
        ExprKind::Member(value) { return numeric_expression_contains(storage, value.base, selected); }
        ExprKind::Index(value) { return numeric_expression_contains(storage, value.base, selected)
            || numeric_expression_contains(storage, value.index, selected); }
        ExprKind::Call(value) { return numeric_arguments_contain(storage, value.arguments, selected); }
        ExprKind::Invoke(value) { return numeric_expression_contains(storage, value.callee, selected)
            || numeric_arguments_contain(storage, value.arguments, selected); }
        ExprKind::Construct(value) { return numeric_arguments_contain(storage, value.arguments, selected); }
        ExprKind::Specialize(value) { return numeric_arguments_contain(storage, value.arguments, selected); }
        ExprKind::Cooperate(value) { return numeric_arguments_contain(storage, value.arguments, selected); }
        ExprKind::Target(value) { return numeric_expression_contains(storage, value.value, selected); }
        ExprKind::Unary(value) { return numeric_expression_contains(storage, value.value, selected); }
        ExprKind::Binary(value) { return numeric_expression_contains(storage, value.left, selected)
            || numeric_expression_contains(storage, value.right, selected); }
        ExprKind::Select(value) { return numeric_expression_contains(storage, value.condition, selected)
            || numeric_expression_contains(storage, value.yes, selected)
            || numeric_expression_contains(storage, value.no, selected); }
        ExprKind::Number(value) {}
        ExprKind::StaticU32(value) {}
        ExprKind::Bool(value) {}
        ExprKind::Name(value) {}
        ExprKind::FunctionRef(value) {}
    }
    return false;
}

extern "C" bool numeric_contains_expression(SignatureNode root, SignatureNode selected) on cpu {
    return address_bits(root.storage.nodes) == address_bits(selected.storage.nodes)
        && numeric_expression_contains(root.storage, ExprRef(root.offset), ExprRef(selected.offset));
}

bool numeric_block_contains(StorageView storage, Slice body, ExprRef selected) on cpu {
    u64 i = 0u64; while (i < body.count) {
        if (numeric_statement_contains(storage, StmtRef(member(storage, body, i)), selected)) { return true; }
        i += 1u64;
    }
    return false;
}

bool numeric_statement_contains(StorageView storage, StmtRef root, ExprRef selected) on cpu {
    match (node<StmtNode>(storage, root.offset).kind) {
        StmtKind::Let(value) { return numeric_expression_contains(storage, value.value, selected); }
        StmtKind::Expr(value) { return numeric_expression_contains(storage, value, selected); }
        StmtKind::Return(value) { return value.present != 0 && numeric_expression_contains(storage, value.value, selected); }
        StmtKind::Seek(value) { return numeric_expression_contains(storage, value.value, selected); }
        StmtKind::Assign(value) { return numeric_expression_contains(storage, value.target, selected)
            || numeric_expression_contains(storage, value.value, selected); }
        StmtKind::If(value) { return numeric_expression_contains(storage, value.condition, selected)
            || numeric_block_contains(storage, value.yes, selected) || numeric_block_contains(storage, value.no, selected); }
        StmtKind::Match(value) {
            if (numeric_expression_contains(storage, value.value, selected)) { return true; }
            u64 i = 0u64; while (i < value.arms.count) {
                if (numeric_block_contains(storage, node<ArmNode>(storage, member(storage, value.arms, i)).body, selected)) { return true; }
                i += 1u64;
            }
        }
        StmtKind::While(value) { return numeric_expression_contains(storage, value.condition, selected)
            || numeric_block_contains(storage, value.body, selected); }
        StmtKind::For(value) { return numeric_expression_contains(storage, value.start, selected)
            || (value.end.present != 0 && numeric_expression_contains(storage, value.end.value, selected))
            || numeric_block_contains(storage, value.body, selected); }
        StmtKind::Parallel(value) {
            ParallelNode domain = node<ParallelNode>(storage, value.domain.offset);
            return (domain.count.present != 0 && numeric_expression_contains(storage, domain.count.value, selected))
                || (domain.selection.present != 0 && numeric_expression_contains(storage, domain.selection.value, selected))
                || numeric_block_contains(storage, value.body, selected);
        }
        StmtKind::Declare(value) {}
        StmtKind::Break() {}
        StmtKind::Continue() {}
        StmtKind::Phase() {}
    }
    return false;
}

extern "C" bool numeric_contains_statement(SignatureNode root, SignatureNode selected) on cpu {
    return address_bits(root.storage.nodes) == address_bits(selected.storage.nodes)
        && numeric_statement_contains(root.storage, StmtRef(root.offset), ExprRef(selected.offset));
}

extern "C" bool numeric_loop_control(StorageView storage, Slice body, u32 seek) on cpu {
    u64 i = 0u64; while (i < body.count) {
        match (node<StmtNode>(storage, member(storage, body, i)).kind) {
            StmtKind::Break() { if (seek == 0) { return true; } }
            StmtKind::Seek(value) { if (seek != 0) { return true; } }
            StmtKind::If(value) {
                if (numeric_loop_control(storage, value.yes, seek) || numeric_loop_control(storage, value.no, seek)) { return true; }
            }
            StmtKind::Match(value) {
                u64 arm = 0u64; while (arm < value.arms.count) {
                    if (numeric_loop_control(storage, node<ArmNode>(storage, member(storage, value.arms, arm)).body, seek)) { return true; }
                    arm += 1u64;
                }
            }
            StmtKind::Declare(value) {}
            StmtKind::Let(value) {}
            StmtKind::Assign(value) {}
            StmtKind::Expr(value) {}
            StmtKind::While(value) {}
            StmtKind::For(value) {}
            StmtKind::Parallel(value) {}
            StmtKind::Return(value) {}
            StmtKind::Continue() {}
            StmtKind::Phase() {}
        }
        i += 1u64;
    }
    return false;
}

NumericPathStep numeric_value(NumericPathStep step, NumericPathAction action, ExprRef value, u32 observe) on cpu {
    NumericPathStep updated = step;
    updated.action = action;
    updated.value = value;
    updated.local = observe;
    return updated;
}

extern "C" NumericPathStep numeric_path_step(StorageView storage, Slice block, NumericPathCursor cursor,
    Truth truth, u32 reaches, Optional<SignatureNode> point) on cpu {
    bootstrap.ast_storage::Text empty = bootstrap.ast_storage::Text(0, Span(0u64, 0u64));
    NumericPathStep step = NumericPathStep(cursor, NumericPathAction::Next, StmtRef(0u64), ExprRef(0u64), Slice(0u64, 0u64),
        Optional<bootstrap.ast_storage::Text>(0, empty), empty, Optional<TypeRef>(0, TypeRef(0u64)), 0);
    if (reaches == 0 || cursor.index >= block.count) { step.action = NumericPathAction::Finish; return step; }
    step.cursor.phase += 1;
    step.statement = StmtRef(member(storage, block, cursor.index));
    SignatureNode source = SignatureNode(storage, step.statement.offset);
    u32 phase = cursor.phase;
    match (node<StmtNode>(storage, source.offset).kind) {
        StmtKind::Declare(value) {
            if (phase == 0) {
                step.action = NumericPathAction::Declare;
                step.binding = Optional<bootstrap.ast_storage::Text>(1, value.name);
                step.annotation = Optional<TypeRef>(1, value.ty);
            }
        }
        StmtKind::Let(value) {
            if (phase == 0) { return numeric_value(step, NumericPathAction::Expression, value.value, 1); }
            if (phase == 1) {
                step.action = NumericPathAction::Let;
                step.binding = Optional<bootstrap.ast_storage::Text>(1, value.name);
                step.annotation = value.annotation;
                step.value = value.value;
            }
        }
        StmtKind::Assign(value) {
            if (phase == 0) { return numeric_value(step, NumericPathAction::Target, value.target, 0); }
            if (phase == 1) { return numeric_value(step, NumericPathAction::Expression, value.value, 1); }
            if (phase == 2) { step.action = NumericPathAction::Assignment; }
        }
        StmtKind::Expr(value) {
            if (phase == 0) { return numeric_value(step, NumericPathAction::Expression, value, 1); }
        }
        StmtKind::If(value) {
            if (phase == 0) { return numeric_value(step, NumericPathAction::Expression, value.condition, 1); }
            if (phase == 1) { return numeric_value(step, NumericPathAction::Condition, value.condition, 0); }
            if (phase == 2 && truth != Truth::False) {
                step.action = NumericPathAction::Yes;
                step.body = value.yes;
                return step;
            }
            if (phase <= 3 && truth != Truth::True) {
                step.action = NumericPathAction::No;
                step.cursor.phase = 4;
                step.body = value.no;
            }
        }
        StmtKind::Match(value) {
            if (phase == 0) { return numeric_value(step, NumericPathAction::Expression, value.value, 2); }
            if (cursor.arm < value.arms.count) {
                ArmNode arm = node<ArmNode>(storage, member(storage, value.arms, cursor.arm));
                step.action = NumericPathAction::Arm;
                step.cursor.arm += 1u64;
                step.body = arm.body;
                step.binding = arm.binding;
                step.variant = arm.variant;
                step.value = value.value;
            }
        }
        StmtKind::Seek(value) {
            if (phase == 0) { return numeric_value(step, NumericPathAction::Expression, value.value, 0); }
            step.action = NumericPathAction::Reenter;
        }
        StmtKind::Continue() { step.action = NumericPathAction::Reenter; }
        StmtKind::Break() { step.action = NumericPathAction::Stop; }
        StmtKind::Return(value) { step.action = NumericPathAction::Stop; }
        StmtKind::While(value) {
            if (phase == 0) {
                if (point.present != 0 && address_bits(storage.nodes) == address_bits(point.value.storage.nodes)
                    && value.condition.offset == point.value.offset) {
                    step.action = NumericPathAction::ObserveLoop;
                    return step;
                }
                step.action = NumericPathAction::Nested;
                step.value = value.condition;
                step.body = value.body;
                step.local = 1;
            } else if (phase == 1 && point.present != 0 && numeric_contains_statement(source, point.value)) {
                step.action = NumericPathAction::NestedPoint;
                step.body = value.body;
            }
        }
        StmtKind::For(value) {
            if (phase == 0) { step.action = NumericPathAction::Nested; }
            if (phase == 1 && point.present != 0 && numeric_contains_statement(source, point.value)) {
                step.action = NumericPathAction::NestedPoint;
                step.body = value.body;
                step.binding = Optional<bootstrap.ast_storage::Text>(1, value.name);
                step.value = value.start;
                step.local = value.end.present != 0 ? 1 : 2;
            }
        }
        StmtKind::Parallel(value) {
            if (phase == 0) { step.action = NumericPathAction::Nested; }
            if (phase == 1 && point.present != 0 && numeric_contains_statement(source, point.value)) {
                step.action = NumericPathAction::NestedPoint;
                step.body = value.body;
                step.binding = Optional<bootstrap.ast_storage::Text>(1, value.name);
                step.variant = node<ParallelNode>(storage, value.domain.offset).dataset;
                step.local = 3;
            }
        }
        StmtKind::Phase() {}
    }
    if (step.action == NumericPathAction::Next) {
        step.cursor = NumericPathCursor(cursor.index + 1u64, 0u64, 0);
    }
    return step;
}

extern "C" NumericRhsStep numeric_rhs_step(SignatureNode source, u32 phase, u32 accepted,
    u32 length, u32 same_name) on cpu {
    StorageView storage = source.storage;
    match (node<StmtNode>(storage, source.offset).kind) {
        StmtKind::Assign(assign) {
            bool equal = spelling(storage, assign.operation, "=");
            if (length != 0) {
                if (!equal) { return NumericRhsStep(0, NumericRhsAction::Unknown, assign.value); }
                match (node<ExprNode>(storage, assign.value.offset).kind) {
                    ExprKind::Call(value) {
                        if (spelling(storage, value.name, "resize") && value.arguments.count == 3u64) {
                            if (phase == 0) { return NumericRhsStep(1, NumericRhsAction::Resize, assign.value); }
                            if (accepted != 0) { return NumericRhsStep(0, NumericRhsAction::Length,
                                ExprRef(member(storage, value.arguments, 1u64))); }
                        }
                    }
                    ExprKind::Specialize(value) {
                        if (spelling(storage, value.name, "dataset") && value.arguments.count != 0u64) {
                            return NumericRhsStep(0, NumericRhsAction::Length, ExprRef(member(storage, value.arguments, 0u64)));
                        }
                    }
                    ExprKind::Name(value) {
                        if (same_name != 0) { return NumericRhsStep(0, NumericRhsAction::Preserve, assign.value); }
                    }
                    ExprKind::Number(unused) {}
                    ExprKind::StaticU32(unused) {}
                    ExprKind::Bool(unused) {}
                    ExprKind::Member(unused) {}
                    ExprKind::Index(unused) {}
                    ExprKind::Invoke(unused) {}
                    ExprKind::Construct(unused) {}
                    ExprKind::FunctionRef(unused) {}
                    ExprKind::Cooperate(unused) {}
                    ExprKind::Target(unused) {}
                    ExprKind::Unary(unused) {}
                    ExprKind::Binary(unused) {}
                    ExprKind::Select(unused) {}
                }
                return NumericRhsStep(0, NumericRhsAction::Unknown, assign.value);
            }
            if (spelling(storage, assign.operation, "+=")) { return NumericRhsStep(0, NumericRhsAction::Shift, assign.value); }
            if (spelling(storage, assign.operation, "-=")) { return NumericRhsStep(0, NumericRhsAction::Subtract, assign.value); }
            if (equal) {
                match (node<ExprNode>(storage, assign.value.offset).kind) {
                    ExprKind::Binary(value) {
                        bool plus = spelling(storage, value.operation, "+");
                        bool minus = spelling(storage, value.operation, "-");
                        if (plus || minus) {
                            if (phase == 0) { return NumericRhsStep(1, NumericRhsAction::Relative, value.left); }
                            if (phase == 1 && accepted != 0) {
                                return NumericRhsStep(0, plus ? NumericRhsAction::Shift : NumericRhsAction::Subtract, value.right);
                            }
                            if (phase == 1) { return NumericRhsStep(2, NumericRhsAction::Relative, value.right); }
                            if (phase == 2 && accepted != 0) {
                                return NumericRhsStep(0, plus ? NumericRhsAction::Shift : NumericRhsAction::Reflect, value.left);
                            }
                        }
                    }
                    ExprKind::Number(unused) {}
                    ExprKind::StaticU32(unused) {}
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
                    ExprKind::Select(unused) {}
                }
                if (phase < 3) { return NumericRhsStep(3, NumericRhsAction::Relative, assign.value); }
                return NumericRhsStep(0, accepted != 0 ? NumericRhsAction::Preserve : NumericRhsAction::Value, assign.value);
            }
        }
        StmtKind::Declare(unused) {}
        StmtKind::Let(unused) {}
        StmtKind::Expr(unused) {}
        StmtKind::If(unused) {}
        StmtKind::Match(unused) {}
        StmtKind::While(unused) {}
        StmtKind::For(unused) {}
        StmtKind::Parallel(unused) {}
        StmtKind::Return(unused) {}
        StmtKind::Break() {}
        StmtKind::Continue() {}
        StmtKind::Phase() {}
        StmtKind::Seek(unused) {}
    }
    return NumericRhsStep(0, NumericRhsAction::Unknown, ExprRef(0u64));
}
