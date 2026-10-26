//! 返回点检查自己的输出与值；退出后只合流真实可继续路径，函数域不结束并行实例。
use super::*;

pub(super) enum Returned {
    Value(Value),
    Dataset(application::Data),
}
#[derive(Clone)]
enum Expected {
    Value(Ty),
    Dataset { ty: Ty, count: Option<u32> },
}
impl Expected {
    fn scalars(&self) -> Vec<Scalar> {
        match self {
            Self::Value(ty) => ty.components.iter().map(|(scalar, _)| *scalar).collect(),
            Self::Dataset { .. } => vec![Scalar::U32, Scalar::U32],
        }
    }
}
impl Returned {
    fn registers(&self) -> Vec<Reg> {
        match self {
            Self::Value(value) => value.regs.clone(),
            Self::Dataset(data) => vec![data.handle.regs[0], data.length.regs[0]],
        }
    }
    fn join(&mut self, other: Self) {
        if let (Self::Dataset(data), Self::Dataset(other)) = (&mut *self, &other) {
            data.handle.proof = bounds::Proof::common(&data.handle, &other.handle);
        }
        let (other, residency) = match &other {
            Self::Value(value) => (value, None),
            Self::Dataset(data) => (&data.length, data.residency),
        };
        let value = match self {
            Self::Value(value) => value,
            Self::Dataset(data) => &mut data.length,
        };
        value.proof = bounds::Proof::common(value, other);
        value.uniform &= other.uniform;
        if value.constant != other.constant {
            value.constant = None;
        }
        if let Self::Dataset(data) = self
            && data.residency != residency
        {
            data.residency = None;
        }
    }
}
struct ReturnExit {
    value: Option<Returned>,
    borrowed: BTreeMap<String, OwnedBinding>,
}
pub(super) struct ReturnContext {
    expected: Option<Expected>,
    scoped: bool,
    slots: Vec<usize>,
    exits: Vec<ReturnExit>,
}
fn early_returns(body: &[Stmt]) -> bool {
    propagates(body)
        || body.iter().enumerate().any(|(i, s)| match &s.kind {
            StmtKind::Return(_) => i + 1 != body.len(),
            StmtKind::If(_, yes, no) | StmtKind::Match(_, _, yes, _, no) => {
                has_return(yes) || has_return(no)
            }
            StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(b),
            _ => false,
        })
}
fn has_return(body: &[Stmt]) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If(_, y, n) | StmtKind::Match(_, _, y, _, n) => has_return(y) || has_return(n),
        StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(b),
        _ => false,
    })
}
fn propagates(body: &[Stmt]) -> bool {
    fn expr(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Call(name, args) => name == "try" || args.iter().any(expr),
            ExprKind::Specialize(_, _, args)
            | ExprKind::Cooperate(_, _, args)
            | ExprKind::Construct(_, args) => args.iter().any(expr),
            ExprKind::Member(e, _) | ExprKind::Unary(_, e) => expr(e),
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => expr(a) || expr(b),
            ExprKind::Select(c, a, b) => expr(c) || expr(a) || expr(b),
            _ => false,
        }
    }
    body.iter().any(|s| match &s.kind {
        StmtKind::Let(_, _, e)
        | StmtKind::Expr(e)
        | StmtKind::Return(Some(e))
        | StmtKind::Seek(_, e) => expr(e),
        StmtKind::Assign(a, _, b) => expr(a) || expr(b),
        StmtKind::If(c, a, b) | StmtKind::Match(c, _, a, _, b) => {
            expr(c) || propagates(a) || propagates(b)
        }
        StmtKind::While(c, body) => expr(c) || propagates(body),
        StmtKind::For(_, a, b, body) => expr(a) || b.as_ref().is_some_and(expr) || propagates(body),
        StmtKind::Parallel(_, domain, body) => {
            domain.count.as_ref().is_some_and(expr)
                || domain.selection.as_ref().is_some_and(expr)
                || propagates(body)
        }
        _ => false,
    })
}
fn owns_exit(code: &[Inst]) -> bool {
    code.iter().any(|i| match i {
        Inst::Exit => true,
        Inst::If { yes, no, .. } => owns_exit(yes) || owns_exit(no),
        Inst::Loop {
            condition, body, ..
        } => owns_exit(condition) || owns_exit(body),
        _ => false,
    })
}
impl Builder<'_> {
    pub(super) fn ordinary_return(
        &self,
        result: Option<Returned>,
        span: Span,
    ) -> Result<Option<Value>> {
        match result {
            Some(Returned::Value(value)) => Ok(Some(value)),
            None => Ok(None),
            Some(Returned::Dataset(_)) => self.fail(
                span,
                "B224",
                "拥有的 dataset 需要资源声明或资源返回位置，不能当作普通值",
            ),
        }
    }
    pub(super) fn return_statement(&mut self, expr: Option<&Expr>, span: Span) -> Result<()> {
        let Some(context) = &self.return_context else {
            return self.fail(
                span,
                "B130",
                "return 只能结束当前普通函数，不能退出 parallel 实例",
            );
        };
        let expected = context.expected.clone();
        let value = match (expected, expr) {
            (Some(Expected::Value(ty)), Some(expr)) => {
                let value = self.expr_expected(expr, Some(&ty))?;
                self.require_same(&ty, &value.ty, expr.span)?;
                Some(Returned::Value(value))
            }
            (Some(Expected::Dataset { ty, count }), Some(expr)) => {
                let mut data = self.application_resource(expr, span)?;
                self.application_data_type(&mut data, &ty, count, span)?;
                data.count = count;
                Some(Returned::Dataset(data))
            }
            (None, None) => None,
            (Some(_), None) => return self.fail(span, "B228", "有返回值函数不能使用 return;"),
            (None, Some(_)) => return self.fail(span, "B228", "void 函数不能返回普通值"),
        };
        self.finish_return(value, span)
    }
    pub(super) fn result_return_type(&self) -> Option<Ty> {
        match self.return_context.as_ref()?.expected.as_ref()? {
            Expected::Value(ty) => Some(ty.clone()),
            _ => None,
        }
    }
    pub(super) fn finish_return(&mut self, mut value: Option<Returned>, span: Span) -> Result<()> {
        if let Some(Returned::Value(value)) = &mut value {
            value.uniform &= self.control_uniform;
            value.proof = bounds::Proof::returning(value, &self.env);
        }
        self.release_application_data(None);
        let mut borrowed = BTreeMap::new();
        let definition = self.definition.clone();
        for param in &definition.function.params {
            if param.ty.dataset
                && param.access == Access::Write
                && let Some(Binding::ApplicationData(data)) = self.env.get(&param.name)
                && !self.application_initialized(data)
            {
                return self.fail(
                    span,
                    "B229",
                    format!("返回前没有完整初始化 write dataset 参数 {}", param.name),
                );
            }
            if matches!(param.access, Access::Write | Access::InOut)
                && !param.ty.dataset
                && !param.ty.tile
            {
                let place = self.place(&Expr {
                    kind: ExprKind::Name(param.name.clone()),
                    span: param.span,
                })?;
                if param.access == Access::Write && !self.is_initialized(&place) {
                    return self.fail(
                        span,
                        "B229",
                        format!("返回前没有完整初始化 write 参数 {}", param.name),
                    );
                }
                let mut owner = self.borrow_owned(place);
                owner.uniform &= self.control_uniform;
                borrowed.insert(param.name.clone(), owner);
            }
        }
        let context = self.return_context.as_mut().unwrap();
        if context.scoped {
            if let Some(value) = &value {
                for (slot, reg) in context.slots.iter().zip(value.registers()) {
                    self.code.push(Inst::StoreLocal(*slot, reg));
                }
            }
            self.code.push(Inst::Exit);
        }
        context.exits.push(ReturnExit { value, borrowed });
        self.flow = Flow::RETURN;
        Ok(())
    }
    pub(super) fn function_body(&mut self, body: &[Stmt]) -> Result<Option<Returned>> {
        let definition = self.definition.clone();
        let name = &definition.function.result;
        let expected = if name.name == "void" && !name.dataset && !name.tile && !name.view {
            None
        } else {
            let ty = self.world.ty(
                name,
                &definition.source,
                definition.function.span,
                &definition.module,
                &definition.imports,
            )?;
            Some(if name.dataset {
                Expected::Dataset {
                    ty,
                    count: self.world.shape(
                        &name.count,
                        &definition.source,
                        definition.function.span,
                        &definition.module,
                        &definition.imports,
                    )?,
                }
            } else {
                Expected::Value(ty)
            })
        };
        let scoped = early_returns(body);
        if scoped && definition.function.kind == FunctionKind::Cooperative {
            return self.fail(
                definition.function.span,
                "B208",
                "cooperative 提前返回的整块收敛语义尚未实现；协作函数保持末尾返回",
            );
        }
        let slots = if scoped {
            expected
                .iter()
                .flat_map(Expected::scalars)
                .map(|scalar| {
                    let slot = self.locals.len();
                    self.locals.push(scalar);
                    slot
                })
                .collect()
        } else {
            Vec::new()
        };
        let initial = self.application_initialization_state();
        let caller = self.return_context.replace(ReturnContext {
            expected,
            scoped,
            slots,
            exits: Vec::new(),
        });
        let flow = std::mem::replace(&mut self.flow, Flow::NEXT);
        let uniform = self.control_uniform;
        let outer = std::mem::take(&mut self.code);
        let mut sequence = Flow::NEXT;
        for statement in body {
            if !sequence.continues() {
                break;
            }
            self.flow = Flow::NEXT;
            self.statement(statement)?;
            sequence = sequence.then(self.flow);
        }
        self.flow = sequence;
        if sequence.continues() {
            if self.return_context.as_ref().unwrap().expected.is_some() {
                return self.fail(
                    definition.function.span,
                    if body.is_empty() { "B132" } else { "B133" },
                    "函数存在未返回普通值的继续路径",
                );
            }
            self.return_statement(None, definition.function.span)?;
        }
        if self.return_context.as_ref().unwrap().exits.is_empty() {
            if self.return_context.as_ref().unwrap().expected.is_some() {
                return self.fail(
                    definition.function.span,
                    "B133",
                    "函数没有可交付普通值的返回路径",
                );
            }
            self.return_statement(None, definition.function.span)?;
        }
        let context = self.return_context.take().unwrap();
        self.return_context = caller;
        self.flow = flow;
        self.control_uniform = uniform;
        let mut code = std::mem::replace(&mut self.code, outer);
        let single_exit = scoped
            && matches!(code.last(), Some(Inst::Exit))
            && !owns_exit(&code[..code.len() - 1]);
        if single_exit {
            code.pop();
        }
        if scoped && !single_exit {
            self.code.push(Inst::Scope(code));
        } else {
            self.code.extend(code);
        }
        self.initialized = initial;
        let mut exits = context.exits.into_iter();
        let first = exits.next().unwrap();
        let mut result = first.value;
        let mut borrowed = first.borrowed;
        for exit in exits {
            if let (Some(value), Some(other)) = (&mut result, exit.value) {
                value.join(other);
            }
            for (name, owner) in &mut borrowed {
                let other = &exit.borrowed[name];
                owner
                    .proof
                    .join(&other.proof, owner.place.ty().components.len(), None);
                owner.uniform &= other.uniform;
            }
        }
        for (name, owner) in borrowed {
            self.initialize(&owner.place);
            self.env.insert(name, Binding::Owned(owner));
        }
        if scoped {
            if let Some(value) = &mut result {
                let registers: Vec<_> = context
                    .slots
                    .iter()
                    .map(|slot| {
                        let reg = self.reg(self.locals[*slot]);
                        self.code.push(Inst::LoadLocal(reg, *slot));
                        reg
                    })
                    .collect();
                match value {
                    Returned::Value(value) => value.regs = registers,
                    Returned::Dataset(data) => {
                        data.handle.regs = vec![registers[0]];
                        data.handle.proof = bounds::Proof::capture(&data.handle);
                        data.length.regs = vec![registers[1]];
                        data.length.proof =
                            bounds::Proof::capture(&data.length).with_domains(Domains::from([
                                IndexDomain::ApplicationLength(data.handle.identity()),
                            ]));
                    }
                }
            }
        }
        Ok(result)
    }
}
