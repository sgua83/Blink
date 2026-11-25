use super::*;
use crate::{
    frontend::{Definition, NameResolution},
    syntax::wire::{ExprRef, Optional, SignatureNode, Slice, StorageView, Text, TypeRef},
};

#[derive(Clone, Copy)]
#[repr(u32)]
enum WritePath { Direct, WhenTrue, WhenFalse, Unknown, TrueMoves, FalseMoves, Range, Elements, Loop }

#[repr(u32)]
enum Truth { Absent, Conditional, False, True }

impl Truth {
    fn of(condition: Option<&bounds::Condition>) -> Self {
        match condition {
            None => Self::Absent,
            Some(condition) => match condition.boolean_constant() {
                Some(false) => Self::False,
                Some(true) => Self::True,
                None => Self::Conditional,
            },
        }
    }
}

#[allow(dead_code)]
#[repr(u32)]
enum Capture { Discard, Control, Right }

#[allow(dead_code)]
#[repr(u32)]
enum ExpressionAction { Finish, Visit, Bool, NameEligible, Name, Comparison, Negate, And, Or, Call, Resource, Move }

#[repr(C)]
struct ExpressionStep {
    phase: u32,
    action: ExpressionAction,
    next_argument: u64,
    child: ExprRef,
    access: Access,
    path: WritePath,
    capture: Capture,
    name: Text,
}

#[allow(dead_code)]
#[repr(u32)]
enum StatementAction {
    Finish, Expression, Block, Declare, Let, Element, MemoryQuery, Memory, Position,
    WhileBounds, Drain, RangeBounds, BindRange, BindElement, BindParallel, BindArm, Unbind, Advance,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct StatementCursor { index: u64, arm: u64, phase: u32 }

#[repr(C)]
struct StatementStep {
    cursor: StatementCursor,
    action: StatementAction,
    path: WritePath,
    access: Access,
    value: ExprRef,
    end: Optional<ExprRef>,
    body: Slice,
    name: Text,
    variant: Text,
    annotation: Optional<TypeRef>,
    local: u32,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum CallForm { Named, Specialized, Cooperative, Indirect }

#[derive(Clone, Copy)]
#[repr(C)]
struct CallSyntax { form: CallForm, name: Text, arguments: Slice, callee: ExprRef }

#[derive(Clone, Copy)]
#[repr(u32)]
enum CallResolution { Missing, Unique, Ambiguous }

#[allow(dead_code)]
#[repr(u32)]
enum CallAction {
    Finish, BoundType, Local, Target, Function, BadSignature, Declared, Instance, MemoryContract,
    ReadOnly, Parameter, Contents, Position, External, Visit, Worker, LengthNumber, LengthName,
    Factory, FactoryEffects, Generator, Advance, BuiltinType, RecordType, ReadPlace,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct CallCursor { index: u64, parameter: u64, phase: u32, signature: u32, default_access: Access }

#[derive(Clone, Copy)]
#[repr(C)]
struct CallFacts {
    current: u32,
    bound_type: u32,
    builtin_type: u32,
    record: u32,
    typed: u32,
    function: u32,
    parameter_count: u64,
    parameter_access: Access,
    resolution: CallResolution,
    declaration: SignatureNode,
    known_memory_write: u32,
    instantiated: u32,
    read_only: u32,
    contents: u32,
    empty: u32,
    initialized_length: u32,
    length: SignatureNode,
    factory_owns: u32,
}

#[repr(C)]
struct CallStep {
    cursor: CallCursor,
    action: CallAction,
    value: ExprRef,
    worker: Text,
    access: Access,
    parameter: u64,
    contents: u32,
    local: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.source_writes::expression_step"]
    fn expression_step(source: SignatureNode, phase: u32, argument: u64, access: Access,
        current: u32, loop_scope: u32, truth: Truth, fact: u32) -> ExpressionStep;
    #[link_name = "bootstrap.source_writes::statement_step"]
    fn statement_step(storage: StorageView, body: Slice, cursor: StatementCursor,
        current: u32, truth: Truth, present: u32) -> StatementStep;
    #[link_name = "bootstrap.source_writes::call_syntax"]
    fn call_syntax(source: SignatureNode) -> CallSyntax;
    #[link_name = "bootstrap.source_writes::call_step"]
    fn call_step(source: SignatureNode, call: CallSyntax, cursor: CallCursor, facts: CallFacts) -> CallStep;
}

// Only the selected write is transformed. The destination remains the original
// consumer (or the existing loop summary while that loop is being collected).
enum Route<'a, 'w> {
    Moves(bounds::Condition),
    Conditional(bounds::Condition),
    Range(Option<(&'a Builder<'w>, &'a bounds::Condition)>, &'a str),
    Elements(&'a str, Expr),
}

struct RoutedSink<'a, 'w> {
    destination: &'a mut dyn WriteSink,
    route: Route<'a, 'w>,
}

fn restrict_position(when: &mut Option<bounds::Condition>, condition: &bounds::Condition) {
    *when = Some(match when.take() {
        Some(previous) => previous.logical(condition.clone(), false),
        None => condition.clone(),
    });
}

impl WriteSink for RoutedSink<'_, '_> {
    fn accept<'a>(&'a mut self, world: &'a World, mut write: Write) -> crate::frontend::Call<'a, Result<()>> {
        unsafe { world.call(async move {
            match &self.route {
                Route::Moves(condition) => write.restrict_move(condition),
                Route::Conditional(condition) => {
                    if let Write::Position { call: None, when, .. } = &mut write {
                        restrict_position(when, condition);
                    }
                    write.restrict_move(condition);
                }
                Route::Range(projection, name) => {
                    write.restrict_move(&bounds::Condition::default());
                    if let Write::Position { place, call: None, when, .. } = &mut write
                        && let Some((builder, range)) = projection
                        && let Some((_, _, Some(selection))) = builder.world.call(builder.loop_write_projection(place,
                            &async |index, dataset| Ok((None, (dataset && matches!(
                                index.kind(), ExprKind::Name(bound) if bound == *name
                            )).then(|| (*range).clone()))))).await?
                    {
                        restrict_position(when, &selection);
                    }
                }
                Route::Elements(name, start) => {
                    write.restrict_move(&bounds::Condition::default());
                    if let Write::Position { place, iterator, .. } = &mut write
                        && assigned_root(place).as_deref() == Some(*name)
                    {
                        *iterator = Some(((*name).to_owned(), start.clone()));
                    }
                }
            }
            self.destination.accept(world, write).await
        }) }
    }
}

fn branch(condition: &Option<bounds::Condition>, negate: bool) -> bounds::Condition {
    let condition = condition.clone().unwrap_or_default();
    if negate { condition.negate() } else { condition }
}

fn position(place: Expr, call: Option<Expr>, contents: bool, local_value: bool) -> Write {
    Write::Position { place, call, contents, local_value, iterator: None, when: None }
}

fn resolve(definition: NameResolution, world: &World) -> (CallResolution, Option<&Definition>) {
    match definition {
        NameResolution::Missing => (CallResolution::Missing, None),
        NameResolution::Ambiguous => (CallResolution::Ambiguous, None),
        NameResolution::Unique(key) => (CallResolution::Unique, Some(&world.functions[&key])),
    }
}

impl Scope<'_, '_> {
    pub(super) async fn writes_expression(&self, expr: &Expr, writes: &mut dyn WriteSink,
        element: Option<(&DataProjection<'_>, &bounds::Clause)>, access: Access
    ) -> Result<Option<bounds::Condition>> {
        let world = self.world;
        let view = expr.source_view();
        let source = expr.signature_node();
        let current = u32::from(self.builder.is_some());
        let loop_scope = u32::from(self.writes.is_some());
        let mut phase = 0;
        let mut argument = 0;
        let mut fact = 0;
        let mut control = None;
        let mut right = None;
        let mut truth = None;
        loop {
            let step = unsafe { expression_step(source, phase, argument, access, current, loop_scope,
                Truth::of(control.as_ref()), fact) };
            phase = step.phase;
            argument = step.next_argument;
            match step.action {
                ExpressionAction::Finish => return Ok(truth),
                ExpressionAction::Visit => {
                    let child = view.node(step.child.offset);
                    let result = match step.path {
                        WritePath::Direct => unsafe { world.call(self.writes_expression(&child, writes, element, step.access)).await }?,
                        path => {
                            let condition = branch(&control, matches!(path, WritePath::FalseMoves));
                            let mut sink = RoutedSink { destination: writes, route: Route::Moves(condition) };
                            unsafe { world.call(self.writes_expression(&child, &mut sink, element, step.access)).await }?
                        }
                    };
                    match step.capture {
                        Capture::Discard => {}
                        Capture::Control => control = result,
                        Capture::Right => right = result,
                    }
                }
                ExpressionAction::Bool => truth = Some(bounds::Condition::constant(step.child.offset != 0)),
                ExpressionAction::NameEligible => {
                    fact = u32::from(!self.builder.unwrap().binding_uses.mutable(view.text(step.name)));
                }
                ExpressionAction::Name | ExpressionAction::Comparison => {
                    if let Some((ty, proof, _)) = unsafe { world.call(self.input_snapshot(
                        expr, self.bindings.len(), Access::Value, element, None
                    )).await }? && ty == Ty::scalar(Scalar::Bool) {
                        truth = proof.component_condition(0).cloned();
                        if matches!(step.action, ExpressionAction::Name) && truth.is_none() {
                            truth = proof.known_value(&ty).map(|bits| bounds::Condition::constant(bits[0] != 0));
                        }
                    }
                }
                ExpressionAction::Negate => truth = control.take().map(bounds::Condition::negate),
                ExpressionAction::And | ExpressionAction::Or => {
                    let condition = control.take().unwrap_or_default().logical(right.take().unwrap_or_default(),
                        matches!(step.action, ExpressionAction::Or));
                    truth = (!condition.unconstrained()).then_some(condition);
                }
                ExpressionAction::Call => unsafe { world.call(self.call_writes(expr, writes, element)).await }?,
                ExpressionAction::Resource => fact = u32::from(unsafe { world.call(self.expression(expr)).await }?
                    .is_some_and(|ty| ty.owns_resources())),
                ExpressionAction::Move => writes.accept(world, Write::Move {
                    place: expr.clone(), when: bounds::Condition::constant(true),
                }).await?,
            }
        }
    }

    pub(super) async fn writes_statements(&mut self, code: &Block, root: &Block, writes: &mut dyn WriteSink) -> Result<()> {
        let view = code.source_view();
        let storage = view.storage();
        let source = code.source_slice();
        let world = self.world;
        let outer = self.bindings.len();
        let builder = self.builder;
        let current = u32::from(builder.is_some());
        let mut cursor = StatementCursor::default();
        let mut condition = None;
        let mut borrow = None;
        let mut range = None;
        let mut present = 0;
        let mut future = Vec::new();
        loop {
            let step = unsafe { statement_step(storage, source, cursor, current, Truth::of(condition.as_ref()), present) };
            cursor = step.cursor;
            match step.action {
                StatementAction::Advance => {},
                StatementAction::Finish => {
                    self.bindings.truncate(outer);
                    return Ok(());
                }
                StatementAction::Expression => {
                    let value = view.node(step.value.offset);
                    condition = if matches!(step.path, WritePath::Loop) {
                        unsafe { world.call(self.writes_expression(&value, &mut async |write: Write| {
                            future.push(write);
                            Ok(())
                        }, None, step.access)).await }?
                    } else {
                        unsafe { world.call(self.writes_expression(&value, writes, None, step.access)).await }?
                    };
                }
                StatementAction::Block => {
                    let body = view.list(step.body);
                    let route = match step.path {
                        WritePath::Direct => {
                            unsafe { world.call(self.writes_statements(&body, root, writes)).await }?;
                            continue;
                        }
                        WritePath::Loop => {
                            unsafe { world.call(self.writes_statements(&body, root, &mut async |write: Write| {
                                future.push(write);
                                Ok(())
                            })).await }?;
                            continue;
                        }
                        WritePath::WhenTrue | WritePath::WhenFalse => {
                            Route::Conditional(branch(&condition, matches!(step.path, WritePath::WhenFalse)))
                        }
                        WritePath::Range => Route::Range(builder.zip(range.as_ref()), view.text(step.name)),
                        WritePath::Elements => Route::Elements(view.text(step.name), view.node(step.value.offset)),
                        _ => Route::Moves(bounds::Condition::default()),
                    };
                    let mut sink = RoutedSink { destination: writes, route };
                    unsafe { world.call(self.writes_statements(&body, root, &mut sink)).await }?;
                }
                StatementAction::Declare => self.bindings.push((view.text(step.name).into(),
                    Local::Typed(view.node(step.annotation.value.offset)))),
                StatementAction::Let => self.bindings.push((view.text(step.name).into(), Local::Initialized(
                    (step.annotation.present != 0).then(|| view.node(step.annotation.value.offset)),
                    view.node(step.value.offset), code.slice(cursor.index as usize + 1..),
                ))),
                StatementAction::Element => {
                    present = u32::from(assigned_root(&view.node(step.value.offset)).is_some_and(|name|
                        self.bindings.iter().rev().find(|(bound, _)| *bound == name)
                            .is_some_and(|(_, local)| matches!(local, Local::NamedElement(_)))));
                }
                StatementAction::MemoryQuery => {
                    let value = view.node(step.value.offset);
                    borrow = unsafe { world.call(self.input_snapshot(&value, self.bindings.len(), Access::Write, None, None)).await }?
                        .map(|(_, _, borrow)| borrow);
                    present = u32::from(borrow.is_some());
                }
                StatementAction::Memory => writes.accept(world,
                    Write::memory(view.node(step.value.offset), borrow.take().unwrap())).await?,
                StatementAction::Position => {
                    let place = view.node(step.value.offset);
                    let local = step.local != 0 && unsafe { world.call(self.local_value_write(&place)).await }? == Some(true);
                    writes.accept(world, position(place, None, false, local)).await?;
                }
                StatementAction::WhileBounds => {
                    let value = view.node(step.value.offset);
                    let body = view.list(step.body);
                    unsafe { world.call(self.while_write_bounds(&value, &body, root, &mut future)).await }?;
                }
                StatementAction::Drain => {
                    for mut write in std::mem::take(&mut future) {
                        write.restrict_move(&bounds::Condition::default());
                        writes.accept(world, write).await?;
                    }
                }
                StatementAction::RangeBounds => {
                    let start = view.node(step.value.offset);
                    let end = view.node(step.end.value.offset);
                    let body = view.list(step.body);
                    range = unsafe { world.call(self.range_write_bounds(&start, &end, &body)).await }?;
                }
                StatementAction::BindRange => self.bindings.push((view.text(step.name).into(), Local::Range)),
                StatementAction::BindElement => self.bindings.push((view.text(step.name).into(),
                    Local::Element(view.node(step.value.offset)))),
                StatementAction::BindParallel => self.bindings.push((view.text(step.name).into(),
                    Local::NamedElement(view.text(step.variant).into()))),
                StatementAction::BindArm => self.bindings.push((view.text(step.name).into(),
                    Local::Payload(view.node(step.value.offset), view.text(step.variant).into()))),
                StatementAction::Unbind => { self.bindings.pop(); }
            }
        }
    }

    async fn call_writes(&self, expr: &Expr, writes: &mut dyn WriteSink,
        element: Option<(&DataProjection<'_>, &bounds::Clause)>
    ) -> Result<()> {
        let world = self.world;
        let lexical = self.lexical;
        let view = expr.source_view();
        let source = expr.signature_node();
        let call = unsafe { call_syntax(source) };
        let name = view.text(call.name);
        let args = view.list(call.arguments);
        let mut cursor = CallCursor { index: 0, parameter: 0, phase: 0, signature: 0, default_access: Access::Value };
        let mut facts = CallFacts {
            current: u32::from(self.builder.is_some()), bound_type: 0, builtin_type: 0, record: 0,
            typed: 0, function: 0, parameter_count: 0, parameter_access: Access::Value,
            resolution: CallResolution::Missing, declaration: source, known_memory_write: 0,
            instantiated: 0, read_only: 0, contents: 0, empty: 0, initialized_length: 0, length: source,
            factory_owns: 0,
        };
        let mut typed = None;
        let mut definition = None;
        let mut instance = None;
        let mut bound_length: Option<&Expr> = None;
        loop {
            let step = unsafe { call_step(source, call, cursor, facts) };
            cursor = step.cursor;
            match step.action {
                CallAction::Advance => {},
                CallAction::Finish => return Ok(()),
                CallAction::BoundType => facts.bound_type = u32::from(lexical.bindings().type_call(name).is_some()),
                CallAction::BuiltinType => facts.builtin_type = u32::from(Ty::builtin(name).is_some()),
                CallAction::RecordType => facts.record = u32::from(!world.resolve(name, lexical.module(), lexical.imports(), false).is_missing()),
                CallAction::Local => {
                    typed = unsafe { world.call(self.named(name, expr.span())).await }?;
                    facts.typed = u32::from(typed.is_some());
                }
                CallAction::Target => {
                    let callee = view.node(step.value.offset);
                    typed = Some(unsafe { world.call(self.expression(&callee)).await }?.ok_or_else(|| Diagnostic {
                        source: lexical.source().clone(), span: callee.span(), code: "B263",
                        message: "不能从原声明与词法来源取得间接调用签名".into(),
                    })?);
                }
                CallAction::Function => {
                    if let Some(function) = &typed.as_ref().unwrap().function {
                        facts.function = 1;
                        facts.parameter_count = function.parameters.len() as u64;
                        facts.known_memory_write = u32::from(outputs::external_memory(function.contracts.iter())
                            .is_some_and(|access| access.writes()));
                    }
                }
                CallAction::BadSignature => return lexical.source().fail(expr.span(), "B263", "间接调用目标需要cfn原生C签名值"),
                CallAction::Declared | CallAction::Worker => {
                    (facts.resolution, definition) = resolve(world.resolve(
                        view.text(step.worker), lexical.module(), lexical.imports(), true
                    ), world);
                    if let Some(definition) = definition {
                        facts.declaration = definition.function.signature_node();
                    }
                }
                CallAction::Instance => {
                    instance = unsafe { world.call(self.call_instance(expr)).await }?;
                    facts.instantiated = u32::from(instance.is_some());
                }
                CallAction::MemoryContract => {
                    let function = &definition.unwrap().function;
                    facts.known_memory_write = u32::from(outputs::external_memory(function.contracts().iter()
                        .filter_map(|contract| {
                            if contract.kind() == ContractKind::Ensures
                                && let crate::syntax::PredicateKind::Memory(access) = contract.predicate().kind()
                            { Some(ContractPredicate::Memory(access)) } else { None }
                        })).is_some_and(|access| access.writes()));
                }
                CallAction::ReadOnly => facts.read_only = u32::from(self.builder.unwrap().functions
                    .effect_read_only(world, &instance.as_ref().unwrap().name)?),
                CallAction::Parameter => facts.parameter_access = typed.as_ref().unwrap().function.as_ref().unwrap()
                    .parameters[step.parameter as usize].0,
                CallAction::Contents => {
                    let place = view.node(step.value.offset);
                    facts.contents = u32::from(unsafe { world.call(self.expression(&place)).await }?
                        .is_some_and(|ty| ty.dataset.is_some()));
                }
                CallAction::Position => {
                    let place = view.node(step.value.offset);
                    let local = step.local != 0 && unsafe { world.call(self.local_value_write(&place)).await }? == Some(true);
                    writes.accept(world, position(place, Some(expr.clone()), step.contents != 0, local)).await?;
                }
                CallAction::ReadPlace => {
                    if let Some(name) = assigned_root(&view.node(step.value.offset)) {
                        self.binding_uses.unwrap().borrow_mut().read_place(name);
                    }
                },
                CallAction::External => {
                    let arguments = match &instance {
                        Some(instance) => unsafe { world.call(self.call_inputs(instance, &args)).await }?,
                        None => BTreeMap::new(),
                    };
                    writes.accept(world, Write::External {
                        call: expr.clone(), instance: instance.take(), arguments, syntax: args.clone(),
                    }).await?;
                }
                CallAction::Visit => {
                    let value = view.node(step.value.offset);
                    unsafe { world.call(self.writes_expression(&value, writes, element, step.access)).await }?;
                }
                CallAction::LengthNumber => {
                    let length = bound_length.cloned().unwrap_or_else(|| view.node(step.value.offset));
                    facts.empty = u32::from(number(lexical.source(), length.span(),
                        length.source_view().text(step.worker))?.1 == 0);
                }
                CallAction::LengthName => {
                    let name = view.text(step.worker);
                    let binding = self.bindings.iter().rev().find(|(bound, _)| *bound == name);
                    match binding {
                        Some((_, Local::Initialized(None, value, _))) if self.builder
                            .is_some_and(|builder| !builder.binding_uses.mutable(name)) =>
                        {
                            bound_length = Some(value);
                            facts.initialized_length = 1;
                            facts.length = value.signature_node();
                        }
                        Some(_) => {}
                        None => facts.empty = u32::from(self.builder.is_some_and(|builder| matches!(
                            builder.env.get(name), Some(Binding::Value(value))
                            if value.ty == Ty::scalar(Scalar::U32) && value.constant.as_ref().is_some_and(|bits| bits.first() == Some(&0))
                        ))),
                    }
                }
                CallAction::Factory => {
                    let factory = view.node(step.value.offset);
                    instance = unsafe { world.call(self.factory_instance(&factory)).await }?;
                    facts.instantiated = u32::from(instance.is_some());
                    if let Some(instance) = &instance {
                        let definition = &instance.definition;
                        let result_name = unsafe { world.call(instance.bindings.type_name(world, &definition.function.result(),
                            (&definition.source, &definition.module, &definition.imports), definition.function.span())).await }?;
                        let ty = unsafe { world.call(world.ty(&result_name, &definition.source, definition.function.span(),
                            &definition.module, &definition.imports)).await }?;
                        facts.factory_owns = u32::from(ty.owns_resources());
                    }
                }
                CallAction::FactoryEffects => facts.read_only = u32::from(self.builder.unwrap().functions
                    .effect_writes_only_references(world, &instance.as_ref().unwrap().name)?),
                CallAction::Generator => writes.accept(world, Write::Generator(view.node(step.value.offset))).await?,
            }
        }
    }

}

impl Scope<'_, '_> {
    async fn while_write_bounds(&self, expr: &Expr, body: &Block, root: &Block, future: &mut [Write]) -> Result<()> {
        let builder = self.builder.unwrap();
        let scope=Scope{bindings:self.bindings.clone(),writes:self.writes,..Scope::current(builder)};
        let mut retained=Vec::new();
        for(binding_at,(name,local))in scope.bindings.iter().enumerate(){
            let Local::Initialized(_,initial,after)=local else{continue;};
            if unsafe { self.world.call(scope.named(name,expr.span())).await }?!=Some(Ty::scalar(Scalar::U32)){continue;}
            let Some((ty,input,_))=unsafe { self.world.call(scope.input_snapshot(initial,binding_at,Access::Value,None,None)).await }?else{continue;};
            if ty!=Ty::scalar(Scalar::U32){continue;}
            let target=builder.env.get(name).and_then(Binding::stored_place).map_or(LoopPlace::Local,|place|LoopPlace::Stored(place,0));
            let mut prefix_scope=Scope{bindings:scope.bindings[..=binding_at].to_vec(),writes:scope.writes,..Scope::current(builder)};
            let mut prefix=None;
            unsafe { builder.world.call(builder.numeric_loop_paths(after,Some(NumericEffect::Assign(input)),&mut None,name,&target,&future,false,&mut prefix_scope,&bounds::Clause::default(),Some(&expr),&mut prefix)).await }?;
            let Some(NumericEffect::Assign(input))=prefix else{continue;};
            let before=unsafe { builder.world.call(builder.numeric_expression_effect(&expr,name,&target,&future)).await }?;
            if !matches!(before,NumericEffect::Shift{low:0,high:0,..}){continue;}
            let guard=unsafe { self.world.call(scope.input_snapshot(&expr,scope.bindings.len(),Access::Value,None,Some(name))).await }?
                .and_then(|(_,proof,_)|proof.component_condition(0).cloned())
                .map(|condition|bounds::scoped_proof(&Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),&condition.assume(&bounds::Clause::default(),true),&BTreeMap::new())).unwrap_or_default();
            let mut numeric_scope=Scope{bindings:scope.bindings.clone(),writes:scope.writes,..Scope::current(builder)};
            let mut continues=None;
            let next=unsafe { builder.world.call(builder.numeric_loop_paths(&body,Some(before),&mut continues,name,&target,&future,false,&mut numeric_scope,&bounds::Clause::default(),None,&mut None)).await }?;
            retained.push((name.clone(),input,guard,join_numeric_paths(next,continues)));
        }
        if scope.writes.is_some() && builder.loop_contexts.last().is_some_and(|context|context.control_source.cursor.is_none()) {
            for(name,binding)in &builder.env {
                if scope.bindings.iter().any(|(bound,_)|*bound==name.as_str()){continue;}
                let Binding::Owned(owner)=binding else{continue;};
                if owner.place.ty()!=&Ty::scalar(Scalar::U32)
                    || !matches!(owner.place.storage(),Place::Local(..)|Place::Reference{..}) {continue;}
                let target=LoopPlace::Stored(owner.place.clone(),0);
                let mut point_scope=Scope{bindings:Vec::new(),writes:scope.writes,..Scope::current(builder)};
                let mut input=None;
                unsafe { builder.world.call(builder.numeric_loop_paths(root,Some(NumericEffect::Assign(owner.proof.clone())),&mut None,name,&target,&future,false,&mut point_scope,&bounds::Clause::default(),Some(&expr),&mut input)).await }?;
                let Some(NumericEffect::Assign(input))=input else{continue;};
                let before=unsafe { builder.world.call(builder.numeric_expression_effect(&expr,name,&target,&future)).await }?;
                if !matches!(before,NumericEffect::Shift{low:0,high:0,..}){continue;}
                let guard=unsafe { self.world.call(scope.input_snapshot(&expr,scope.bindings.len(),Access::Value,None,Some(name))).await }?
                    .and_then(|(_,proof,_)|proof.component_condition(0).cloned())
                    .map(|condition|bounds::scoped_proof(&Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0)),&condition.assume(&bounds::Clause::default(),true),&BTreeMap::new())).unwrap_or_default();
                let mut numeric_scope=Scope{bindings:scope.bindings.clone(),writes:scope.writes,..Scope::current(builder)};
                let mut continues=None;
                let next=unsafe { builder.world.call(builder.numeric_loop_paths(&body,Some(before),&mut continues,name,&target,&future,false,&mut numeric_scope,&bounds::Clause::default(),None,&mut None)).await }?;
                retained.push((name.as_str().to_string(),input,guard,join_numeric_paths(next,continues)));
            }
        }
        let iterations=retained.iter().filter_map(|(_,input,guard,effect)|match effect{None=>Some(1),Some(effect)=>effect.iteration_bound(input,guard)}).min();
        for at in 0..future.len(){
            let selection=if let Write::Position{place,call:None,..}=&future[at]{
                unsafe { builder.world.call(builder.loop_write_projection(place,&async |index,dataset|{
                    let operand=builder.loop_index_operand(index,&future)?;
                    let ExprKind::Name(name)=index.kind() else{return Ok((operand,None));};
                    if !dataset||unsafe { self.world.call(scope.named(name,index.span())).await }?!=Some(Ty::scalar(Scalar::U32)){return Ok((operand,None));}
                    let mut condition=unsafe { self.world.call(scope.input_snapshot(&expr,scope.bindings.len(),Access::Value,None,Some(name))).await }?.and_then(|(_,proof,_)|proof.component_condition(0).cloned());
                    if iterations==Some(0){return Ok((operand,Some(bounds::Condition::constant(false))));}
                    let range=iterations.and_then(|iterations|retained.iter().find(|(bound,_,_,_)|*bound==name).and_then(|(_,input,_,effect)|match effect {
                        None=>Some((input.unsigned_minimum(0),input.unsigned_maximum(0))),
                        Some(effect)=>effect.bounded_ranges(input,iterations).map(|range|range.0),
                    }));
                    if let Some((low,high))=range{
                        let ordinal=bounds::Operand::Value(ValueIdentity::ElementIndex(0));
                        let proof=bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0));
                        for(value,lower)in [(low,true),(high,false)]{
                            let bound=bounds::Proof::unsigned(value,value);let operand=bounds::Operand::Constant(Scalar::U32,Bits::from(value));
                            let limit=if lower{bounds::Condition::index_below((operand,&bound),(ordinal,&proof),false)}else{bounds::Condition::index_below((ordinal,&proof),(operand,&bound),false)};
                            condition=Some(match condition{Some(previous)=>previous.logical(limit,false),None=>limit});
                        }
                    }
                    let Some(condition)=condition else{return Ok((operand,None));};
                    let target=builder.env.get(name).and_then(Binding::stored_place).map_or(LoopPlace::Local,|place|LoopPlace::Stored(place,0));
                    let mut numeric_scope=Scope{bindings:scope.bindings.clone(),writes:scope.writes,..Scope::current(builder)};
                    let mut observed=None;let before=unsafe { builder.world.call(builder.numeric_expression_effect(&expr,name,&target,&future)).await }?;
                    unsafe { builder.world.call(builder.numeric_loop_paths(&body,Some(before),&mut None,name,&target,&future,false,&mut numeric_scope,&bounds::Clause::default(),Some(place),&mut observed)).await }?;
                    let condition=match observed {
                        None=>Some(bounds::Condition::constant(false)),
                        Some(NumericEffect::Shift{low:0,high:0,..})=>Some(condition),
                        _=>None,
                    };
                    Ok((operand,condition))
                })).await }?.and_then(|(_,_,selection)|selection)
            }else{None};
            if let(Some(selection),Write::Position{when,..})=(selection,&mut future[at]){
                *when=Some(match when.take(){Some(previous)=>previous.logical(selection,false),None=>selection});
            }
        }
        Ok(())
    }

    async fn range_write_bounds(&self, start: &Expr, end: &Expr, body: &Block) -> Result<Option<bounds::Condition>> {
        let mut range=None::<bounds::Condition>;
        {
            let ordinal=bounds::Operand::Value(ValueIdentity::ElementIndex(0));
            let proof=bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(0));
            for(expr,lower)in [(start,true),(end,false)] {
                if lower&&bounds::seeks(&body){continue;}
                let Some((ty,bound,_))=unsafe { self.world.call(self.input_snapshot(&expr,self.bindings.len(),Access::Value,None,None)).await }?else{continue;};
                if ty!=Ty::scalar(Scalar::U32){continue;}
                let operand=bound.known_value(&ty).map(|bits|bounds::Operand::Constant(Scalar::U32,bits[0])).or_else(||bound.known_identity(0).map(bounds::Operand::Value));
                let Some(operand)=operand else{continue;};
                let condition=if lower{bounds::Condition::index_below((operand,&bound),(ordinal,&proof),false)}
                    else{bounds::Condition::index_below((ordinal,&proof),(operand,&bound),true)};
                range=Some(match range{Some(previous)=>previous.logical(condition,false),None=>condition});
            }
        }
        Ok(range)
    }
}
