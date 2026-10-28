//! 返回点检查自己的输出与值；退出后只合流真实可继续路径，函数域不结束并行实例。
use super::*;

#[derive(Clone)]
pub(super) enum Returned {
    Unit,
    Value(Value),
    Dataset(application::Data),
    OwnedValue(owned_values::OwnedValue),
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
    pub(super) fn view_value(&self)->Option<&Value>{match self{Self::Value(value)=>Some(value),Self::OwnedValue(value)=>Some(&value.value),_=>None}}
    pub(super) fn view_value_mut(&mut self)->Option<&mut Value>{match self{Self::Value(value)=>Some(value),Self::OwnedValue(value)=>Some(&mut value.value),_=>None}}
    pub(super) fn bind_outputs(
        &mut self,
        registers: Option<&[Reg]>,
        bindings: &BTreeMap<ValueIdentity, ValueIdentity>,
        aliases: &bounds::Clause,
    ) {
        fn value(
            value: &mut Value,
            registers: Option<&[Reg]>,
            bindings: &BTreeMap<ValueIdentity, ValueIdentity>,
            aliases: &bounds::Clause,
        ) {
            value.proof = bounds::Proof::capture(value);
            value
                .proof
                .bind_outputs(&value.ty, registers, bindings, aliases);
        }
        match self {
            Self::Unit => {}
            Self::Value(v) => value(v, registers, bindings, aliases),
            Self::OwnedValue(v) => {
                value(&mut v.value, registers, bindings, aliases);
                for payload in v.payloads.iter_mut().flatten() {
                    payload.bind_outputs(None, bindings, aliases);
                }
            }
            Self::Dataset(v) => {
                v.residency=v.residency.output_bound(bindings);
                value(&mut v.handle, registers.map(|r| &r[..1]), bindings, aliases);
                value(&mut v.length, registers.map(|r| &r[1..]), bindings, aliases);
            }
        }
    }
    pub(super) fn registers(&self) -> Vec<Reg> {
        match self {
            Self::Unit => Vec::new(),
            Self::OwnedValue(value) => value.value.regs.clone(),
            Self::Value(value) => value.regs.clone(),
            Self::Dataset(data) => vec![data.handle.regs[0], data.length.regs[0]],
        }
    }
    pub(super) fn join_locations(&mut self,other:Self,choice:&bounds::Condition){
        fn restore(target:&mut Returned,a:&Returned,b:&Returned,choice:&bounds::Condition){match(target,a,b){
            (Returned::Dataset(target),Returned::Dataset(a),Returned::Dataset(b))=>target.residency=residency::Residency::conditional(choice.clone(),a.residency.clone(),b.residency.clone()),
            (Returned::OwnedValue(target),Returned::OwnedValue(a),Returned::OwnedValue(b))=>for(index,payload)in target.payloads.iter_mut().enumerate(){if let(Some(target),Some(a),Some(b))=(payload.as_deref_mut(),a.payloads[index].as_deref(),b.payloads[index].as_deref()){restore(target,a,b,choice);}},
            _=>{},
        }}
        let before=self.clone();self.join(other.clone());restore(self,&before,&other,choice);
    }
    pub(super) fn join(&mut self, other: Self) {
        if matches!(self, Self::Unit) {
            return;
        }
        if let Self::OwnedValue(value) = self {
            let Self::OwnedValue(other) = other else {
                unreachable!()
            };
            value.join(other);
            return;
        }
        if let (Self::Dataset(data), Self::Dataset(other)) = (&mut *self, &other) {
            if data.ownership != other.ownership {
                data.ownership = application::Ownership::Moved;
            }
            data.handle.proof = bounds::Proof::common(&data.handle, &other.handle);
        }
        let (other, residency) = match &other {
            Self::Unit | Self::OwnedValue(_) => unreachable!(),
            Self::Value(value) => (value, None),
            Self::Dataset(data) => (&data.length, Some(data.residency.clone())),
        };
        let value = match self {
            Self::Unit | Self::OwnedValue(_) => unreachable!(),
            Self::Value(value) => value,
            Self::Dataset(data) => &mut data.length,
        };
        value.proof = bounds::Proof::common(value, other);
        value.uniform &= other.uniform;
        if value.constant != other.constant {
            value.constant = None;
        }
        if let Self::Dataset(data) = self {
            data.residency.join(&residency.unwrap());
        }
    }
}
struct ReturnExit {
    path: bounds::Clause,
    value: Option<Returned>,
    borrowed: Result<BTreeMap<String, Returned>>,
}
pub(super) struct ReturnContext {
    expected: Option<Expected>,
    scoped: bool,
    slots: Vec<usize>,
    exits: Vec<ReturnExit>,
}
impl ReturnContext {
    pub(super) fn exit_count(&self) -> usize {
        self.exits.len()
    }
    pub(super) fn select_exits(&mut self, first: usize, yes_end: usize, truth: bool) {
        if truth {
            self.exits.truncate(yes_end);
        } else {
            self.exits.drain(first..yes_end);
        }
    }
}
fn early_returns(body: &[Stmt]) -> bool {
    propagates(body)
        || body.iter().enumerate().any(|(i, s)| match &s.kind {
            StmtKind::Return(_) => i + 1 != body.len(),
            StmtKind::If(_, yes, no) => {
                has_return(yes) || has_return(no)
            }
            StmtKind::Match(_, arms) => arms.iter().any(|arm|has_return(&arm.body)),
            StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(b),
            _ => false,
        })
}
fn has_return(body: &[Stmt]) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If(_, y, n) => has_return(y) || has_return(n),
        StmtKind::Match(_, arms) => arms.iter().any(|arm|has_return(&arm.body)),
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
            ExprKind::Target(_, e) | ExprKind::Member(e, _) | ExprKind::Unary(_, e) => expr(e),
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
        StmtKind::If(c, a, b) => {
            expr(c) || propagates(a) || propagates(b)
        }
        StmtKind::Match(value, arms) => expr(value) || arms.iter().any(|arm|propagates(&arm.body)),
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
    pub(super) fn written_model(&mut self,name:&str,span:Span)->Result<Returned> {
        match self.env.get(name).cloned().unwrap() {
            Binding::Owned(owner)=>{
                if !self.is_initialized(&owner.place){return self.fail(span,"B229",format!("返回前没有完整初始化独占参数 {name}"));}
                let regs=owner.place.ty().components.iter().map(|(scalar,_)|self.reg(*scalar)).collect();
                Ok(Returned::Value(Value{ty:owner.place.ty().clone(),regs,proof:owner.proof,uniform:owner.uniform,literal:false,constant:None}))
            }
            Binding::OwnedValue(value)=>{
                let mut model=Returned::OwnedValue(value);self.resolve_owned_model(&mut model,None);self.prepare_owned_read(&model);let Returned::OwnedValue(mut value)=model else{unreachable!()};
                if !value.all_available(){return self.fail(span,"B229",format!("返回前没有归还独占参数 {name} 的全部活动拥有资源"));}
                let place=value.place().unwrap();
                if !value.value.ty.has_variants() && !self.is_initialized(&place){return self.fail(span,"B229",format!("返回前没有完整初始化独占参数 {name}"));}
                self.load_owned_value(&mut value,span)?;
                let mut model=Returned::OwnedValue(value);self.require_returned_initialized(&mut model,span)?;Ok(model)
            }
            _=>unreachable!(),
        }
    }
    fn bind_return_exit(
        &mut self,
        value: &mut Option<Returned>,
        borrowed: &mut BTreeMap<String, Returned>,
        path: &mut bounds::Clause,
        registers: &[Reg],
        written: &BTreeMap<String, Vec<Reg>>,
        entry: usize,
    ) {
        let returned = value.as_ref().map(|v| self.returned_storage(v));
        let proof = returned.as_ref().map(bounds::Proof::capture);
        let mut bindings = BTreeMap::new();
        if let Some(proof) = &proof {
            proof.output_bindings(registers, entry, &mut bindings);
        }
        for (name, owner) in borrowed.iter() {
            bounds::Proof::capture(&self.returned_storage(owner)).output_bindings(&written[name],entry,&mut bindings);
        }
        let mut aliases = bounds::Clause::default();
        if let Some(proof) = &proof {
            aliases = bounds::Clause::both(
                &aliases,
                &proof.output_aliases(registers, &self.registers, &bindings),
            );
        }
        for (name, owner) in borrowed.iter() {
            aliases = bounds::Clause::both(
                &aliases,
                &bounds::Proof::capture(&self.returned_storage(owner)).output_aliases(&written[name],&self.registers,&bindings),
            );
        }
        self.view_model.parameters.extend(bindings.clone());
        *path = bounds::Clause::both(&path.output_bound(&bindings), &aliases);
        if let Some(value) = value {
            value.bind_outputs(Some(registers), &bindings, &aliases);
        }
        for (name, owner) in borrowed.iter_mut() {
            owner.bind_outputs(Some(&written[name]),&bindings,&aliases);
        }
    }
    pub(super) fn ordinary_return(
        &self,
        result: Option<Returned>,
        span: Span,
    ) -> Result<Option<Value>> {
        match result {
            Some(Returned::Unit) => Ok(None),
            Some(Returned::OwnedValue(_)) => self.fail(
                span,
                "B224",
                "拥有结果不能按普通值复制；使用绑定、match、try或拥有参数交接",
            ),
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
        if self.definition.function.result.is_never() {
            if let Some(expr) = expr {
                self.evaluate_returned(expr, None)?;
                if !self.flow.continues() {
                    return Ok(());
                }
            }
            return self.fail(span, "B228", "never 函数不能正常返回");
        }
        let expected = context.expected.clone();
        let value = match (expected, expr) {
            (Some(Expected::Value(ty)), Some(expr)) => {
                let result = self.evaluate_returned(expr, Some(&ty))?;
                if !self.flow.continues() {
                    return Ok(());
                }
                let mut value = result.ok_or_else(|| Diagnostic {
                    source: self.definition.source.clone(),
                    span,
                    code: "B228",
                    message: "返回需要值或拥有资源".into(),
                })?;
                self.check_returned_type(&mut value, &ty, expr.span)?;
                Some(value)
            }
            (Some(Expected::Dataset { ty, count }), Some(expr)) => {
                let returned=self.evaluate_returned(expr,Some(&Ty::owned_dataset(ty.clone(),count)))?;
                if !self.flow.continues(){return Ok(());}
                let Some(Returned::Dataset(mut data))=returned else{return self.fail(span,"B224","返回需要实际拥有的dataset")};
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
        if let Some(value)=&mut value {self.require_returned_initialized(value,span)?;}
        if let Some(Returned::Value(value)) = &mut value {
            self.require_view_return(value, span)?;
            value.uniform &= self.control_uniform;
            value.proof = bounds::Proof::returning(value, &self.env, &self.path);
        }
        if let Some(Returned::OwnedValue(value)) = &mut value {
            self.require_view_return(&value.value,span)?;
            value.value.proof = bounds::Proof::returning(&value.value, &self.env, &self.path);
        }
        let borrowed=(||->Result<BTreeMap<String,Returned>> {
            let mut borrowed=BTreeMap::new();let definition=self.definition.clone();
            for param in &definition.function.params {
                if param.ty.dataset && param.access==Access::Write && let Some(Binding::ApplicationData(data))=self.env.get(&param.name) && !self.application_initialized(data) {
                    return self.fail(span,"B229",format!("返回前没有完整初始化 write dataset 参数 {}",param.name));
                }
                if matches!(param.access,Access::Write|Access::InOut) && !param.ty.dataset && !param.ty.tile {
                    let mut model=self.written_model(&param.name,span)?;
                    if let Some(value)=model.view_value_mut(){value.uniform&=self.control_uniform;value.proof.under_path(&value.ty,&self.path);}
                    if let Some(value)=model.view_value(){self.require_view_return(value,span)?;}
                    borrowed.insert(param.name.clone(),model);
                }
            }
            Ok(borrowed)
        })();
        self.release_application_data(None)?;
        let context = self.return_context.as_mut().unwrap();
        if context.scoped {
            if let Some(value) = &value {
                for (slot, reg) in context.slots.iter().zip(value.registers()) {
                    self.code.push(Inst::StoreLocal(*slot, reg));
                }
            }
            self.code.push(Inst::Exit);
        }
        context.exits.push(ReturnExit {
            path: self.path.clone(),
            value,
            borrowed,
        });
        self.flow = Flow::RETURN;
        Ok(())
    }
    pub(super) fn function_body(&mut self, body: &[Stmt]) -> Result<(Option<Returned>,Vec<Option<Returned>>)> {
        let entry = self.registers.len();
        let definition = self.definition.clone();
        let name = &definition.function.result;
        let expected = if (name.name == "void" && !name.dataset && !name.tile && !name.view)
            || name.is_never()
        {
            None
        } else {
            let ty = self.world.ty(
                name,
                &definition.source,
                definition.function.span,
                &definition.module,
                &definition.imports,
            )?;
            if ty.owns_resources()
                && (name.dataset
                    || definition.function.kind != FunctionKind::Value
                    || definition.function.target != Target::Cpu)
            {
                return self.fail(
                    definition.function.span,
                    "B224",
                    "拥有结果由普通CPU函数返回；并行元素与通用拥有聚合尚未实现",
                );
            }
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
            self.view_uses.after(statement.span);
            sequence = sequence.then(self.flow);
        }
        self.flow = sequence;
        if name.is_never() && sequence.continues() {
            return self.fail(
                definition.function.span,
                "B133",
                "never 函数存在可以正常结束的路径",
            );
        }
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
        let Some(first) = exits.next() else {
            self.flow = Flow::STOP;
            self.path = bounds::Clause::impossible();
            return Ok((None,vec![None;definition.function.params.len()]));
        };
        let mut path = first.path;
        let mut result = first.value;
        let mut borrowed = first.borrowed?;
        let registers: Vec<_> = if scoped {
            context
                .slots
                .iter()
                .map(|slot| {
                    let reg = self.reg(self.locals[*slot]);
                    self.code.push(Inst::LoadLocal(reg, *slot));
                    reg
                })
                .collect()
        } else {
            result.as_ref().map(Returned::registers).unwrap_or_default()
        };
        let written: BTreeMap<_, _> = borrowed
            .iter()
            .map(|(name, owner)| {
                (
                    name.clone(),
                    self.returned_storage(owner).ty.components
                        .iter()
                        .map(|(scalar, _)| self.reg(*scalar))
                        .collect(),
                )
            })
            .collect();
        self.bind_return_exit(
            &mut result,
            &mut borrowed,
            &mut path,
            &registers,
            &written,
            entry,
        );
        self.path = path;
        for exit in exits {
            let mut path = exit.path;
            let mut other = exit.value;
            let mut other_borrowed = exit.borrowed?;
            self.bind_return_exit(
                &mut other,
                &mut other_borrowed,
                &mut path,
                &registers,
                &written,
                entry,
            );
            let choice=bounds::Condition::paths(self.path.clone(),path.clone());
            self.path = bounds::Clause::either(&self.path, &path);
            if let (Some(value), Some(other)) = (&mut result, other) {
                value.join_locations(other,&choice);
            }
            for (name, owner) in &mut borrowed {
                let other = &other_borrowed[name];
                owner.join_locations(other.clone(),&choice);
            }
        }
        // 出口已分别检查并合流；不能再用最后解析分支的环境重建输出。
        for(name,model)in &mut borrowed{native_function::rebind_model(model,written[name].clone());}
        let output_models=definition.function.params.iter().map(|parameter|borrowed.get(&parameter.name).cloned()).collect();
        for(name,model)in borrowed {
            let binding=match model {
                Returned::Value(value)=>{let Some(Binding::Owned(mut owner))=self.env.get(&name).cloned()else{unreachable!()};owner.proof=value.proof;owner.uniform=value.uniform;self.initialize(&owner.place);Binding::Owned(owner)}
                Returned::OwnedValue(mut value)=>{let place=self.env[&name].stored_place().unwrap();self.initialize(&place);value.place=Some(place);Binding::OwnedValue(value)}
                _=>unreachable!(),
            };
            self.env.insert(name,binding);
        }
        if scoped {
            if let Some(value) = &mut result {
                self.rebind_returned(value, registers);
            }
        }
        Ok((result,output_models))
    }
}

crate::module::codec::variants!(Returned{0=>Unit,1=>Value(value),2=>Dataset(data),3=>OwnedValue(value)});
