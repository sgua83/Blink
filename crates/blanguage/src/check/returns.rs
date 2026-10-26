//! 返回点检查自己的输出与值；退出后只合流真实可继续路径，函数域不结束并行实例。
use super::*;

struct ReturnExit {
    value: Option<Value>,
    borrowed: BTreeMap<String, OwnedBinding>,
}
pub(super) struct ReturnContext {
    expected: Option<Ty>,
    scoped: bool,
    slots: Vec<usize>,
    exits: Vec<ReturnExit>,
}
fn early_returns(body: &[Stmt]) -> bool {
    body.iter().enumerate().any(|(i, s)| match &s.kind {
        StmtKind::Return(_) => i + 1 != body.len(),
        StmtKind::If(_, yes, no) => has_return(yes) || has_return(no),
        StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(b),
        _ => false,
    })
}
fn has_return(body: &[Stmt]) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If(_, y, n) => has_return(y) || has_return(n),
        StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(b),
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
    pub(super) fn return_statement(&mut self, expr: Option<&Expr>, span: Span) -> Result<()> {
        let Some(context) = &self.return_context else {
            return self.fail(
                span,
                "B130",
                "return 只能结束当前普通函数，不能退出 parallel 实例",
            );
        };
        let expected = context.expected.clone();
        let mut value = match (expected, expr) {
            (Some(ty), Some(expr)) => {
                let value = self.expr(expr)?;
                self.require_same(&ty, &value.ty, expr.span)?;
                Some(value)
            }
            (None, None) => None,
            (Some(_), None) => return self.fail(span, "B228", "有返回值函数不能使用 return;"),
            (None, Some(_)) => return self.fail(span, "B228", "void 函数不能返回普通值"),
        };
        if let Some(value) = &mut value {
            value.uniform &= self.control_uniform;
            value.proof = bounds::Proof::returning(value, &self.env);
        }
        let mut borrowed = BTreeMap::new();
        let definition = self.definition.clone();
        for param in &definition.function.params {
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
                for (slot, reg) in context.slots.iter().zip(&value.regs) {
                    self.code.push(Inst::StoreLocal(*slot, *reg));
                }
            }
            self.code.push(Inst::Exit);
        }
        context.exits.push(ReturnExit { value, borrowed });
        self.flow = Flow::RETURN;
        Ok(())
    }
    pub(super) fn function_body(&mut self, body: &[Stmt]) -> Result<Option<Value>> {
        let definition = self.definition.clone();
        let name = &definition.function.result;
        let expected = if name.name == "void" && !name.dataset && !name.tile && !name.view {
            None
        } else {
            Some(self.world.ty(
                name,
                &definition.source,
                definition.function.span,
                &definition.module,
                &definition.imports,
            )?)
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
                .flat_map(|t| &t.components)
                .map(|(scalar, _)| {
                    let slot = self.locals.len();
                    self.locals.push(*scalar);
                    slot
                })
                .collect()
        } else {
            Vec::new()
        };
        let initial = self.initialized.clone();
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
                value.proof = bounds::Proof::common(value, &other);
                value.uniform &= other.uniform;
                if value.constant != other.constant {
                    value.constant = None;
                }
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
                value.regs = context
                    .slots
                    .iter()
                    .zip(&value.ty.components)
                    .map(|(slot, (scalar, _))| {
                        let reg = self.reg(*scalar);
                        self.code.push(Inst::LoadLocal(reg, *slot));
                        reg
                    })
                    .collect();
            }
        }
        Ok(result)
    }
}
