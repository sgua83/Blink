//! 返回点检查自己的输出与值；退出后只合流真实可继续路径，函数域不结束并行实例。
use super::*;

#[derive(Clone)]
pub(super) enum Returned {
    Unit,
    // 跨阶段结果移动唯一载荷；普通表达式仍直接使用 Value。
    Value(Box<Value>),
    Dataset(application::Data),
    OwnedValue(owned_values::OwnedValue),
}
#[derive(Clone)]
enum Expected {
    Void,
    Never,
    Value(Ty),
    Dataset { ty: Ty, count: Option<u32> },
}
impl Expected {
    fn scalars(&self) -> Vec<Scalar> {
        match self {
            Self::Void | Self::Never => Vec::new(),
            Self::Value(ty) => ty.components.iter().map(|(scalar, _)| *scalar).collect(),
            Self::Dataset { .. } => vec![Scalar::U32, Scalar::U32],
        }
    }
}
impl Returned {
    pub(super) fn value(value: Value) -> Self { Self::Value(Box::new(value)) }
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
                v.residency=v.residency.output_bound(bindings).with_aliases(aliases);
                for fact in v.prefixes.values_mut(){*fact=fact.output_bound(bindings);}
                value(&mut v.root, registers.map(|r| &r[..1]), bindings, aliases);
                value(&mut v.length, registers.map(|r| &r[1..]), bindings, aliases);
                v.elements=std::mem::take(&mut v.elements).map(&mut |mut elements|{
                    if let Some(value)=elements.view_value_mut(){
                        value.proof.origins.resize_with(value.regs.len(),Vec::new);
                        for(component,origins)in value.proof.origins.iter_mut().enumerate(){if origins.is_empty(){origins.push(ValueIdentity::ElementComponent(component));}}
                    }
                    elements.bind_outputs(None,bindings,aliases);elements
                },&mut |when|when.output_bound(bindings).with_aliases(aliases));
            }
        }
    }
    pub(super) fn registers(&self) -> Vec<Reg> {
        match self {
            Self::Unit => Vec::new(),
            Self::OwnedValue(value) => value.value.regs.clone(),
            Self::Value(value) => value.regs.clone(),
            Self::Dataset(data) => vec![data.root.regs[0], data.length.regs[0]],
        }
    }
    // Data.elements contains logical model slots, not registers of the current function.
    pub(super) fn join(&mut self, mut other: Self, stored: bool, choice: Option<&bounds::Condition>) {
        if matches!(self, Self::Unit) {
            return;
        }
        if let Self::OwnedValue(value) = self {
            let Self::OwnedValue(other) = other else {
                unreachable!()
            };
            value.join(other,stored,choice);
            return;
        }
        if let (Self::Dataset(data), Self::Dataset(other)) = (&mut *self, &mut other) {
            let borrows=choice.map(|choice| {
                let a=view::resource_borrow(data);let b=view::resource_borrow(other);
                data.root.proof=std::mem::take(&mut data.root.proof).with_view_borrow(0,view::Borrow::default());
                other.root.proof=std::mem::take(&mut other.root.proof).with_view_borrow(0,view::Borrow::default());
                (choice,a,b)
            });
            if data.ownership != other.ownership {
                data.ownership = application::Ownership::Moved;
            }
            data.root.proof = if stored { let mut proof=data.root.proof.clone(); proof.join_condition(&other.root.proof,data.root.regs.len(),None); proof } else { bounds::Proof::common(&data.root, &other.root,None,None) };
            if let Some((choice,a,b))=borrows {
                data.root.proof=std::mem::take(&mut data.root.proof).with_view_borrow(0,view::Borrow::selected(choice,&a,&b));
            }
            data.elements=if data.length.maximum()==0{other.elements.clone()}
                else if other.length.maximum()==0{std::mem::take(&mut data.elements)}
                else{match choice {
                    Some(choice)=>std::mem::take(&mut data.elements).selected(other.elements.clone(),choice),
                    None=>std::mem::take(&mut data.elements).common(other.elements.clone()),
                }};
            data.prefixes=prefix::join_fields(&data.prefixes,&other.prefixes,choice);
        }
        let (other, residency) = match &other {
            Self::Unit | Self::OwnedValue(_) => unreachable!(),
            Self::Value(value) => (value.as_ref(), None),
            Self::Dataset(data) => (&data.length, Some(data.residency.clone())),
        };
        let value = match self {
            Self::Unit | Self::OwnedValue(_) => unreachable!(),
            Self::Value(value) => value.as_mut(),
            Self::Dataset(data) => &mut data.length,
        };
        value.proof = if stored { let mut proof=value.proof.clone(); proof.join_condition(&other.proof,value.regs.len(),choice); proof } else { bounds::Proof::common(value, other,choice,None) };
        value.uniform &= other.uniform;
        if value.constant != other.constant {
            value.constant = None;
        }
        if let Self::Dataset(data) = self {
            match choice {
                Some(choice)=>data.residency=residency::Residency::conditional(choice.clone(),data.residency.clone(),residency.unwrap()),
                None=>data.residency.join(&residency.unwrap()),
            }
        }
    }
}
struct ReturnExit {
    path: bounds::Clause,
    value: Option<Returned>,
    borrowed: Result<BTreeMap<String, Returned>>,
    initialized: Vec<address::InitializationEffect>,
}
pub(super) struct ReturnContext {
    instance: crate::generics::Instance,
    expected: Arc<Expected>,
    scoped: bool,
    slots: Vec<usize>,
    exits: Vec<ReturnExit>,
    inputs: BTreeMap<String, Value>,
}
// 外层函数体的原状态按值交接，不复制当前返回模型或资源树。
struct BodyState {
    entry: usize,
    scoped: bool,
    initial: BTreeSet<Position>,
    caller: Option<ReturnContext>,
    flow: Flow,
    uniform: bool,
    code_start: usize,
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
fn early_returns(body: &Block) -> bool {
    propagates(body)
        || body.iter().enumerate().any(|(i, s)| match s.kind() {
            StmtKind::Return(_) => i + 1 != body.len(),
            StmtKind::If(_, yes, no) => {
                has_return(&yes) || has_return(&no)
            }
            StmtKind::Match(_, arms) => arms.iter().any(|arm|has_return(&arm.body())),
            StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(&b),
            _ => false,
        })
}
fn has_return(body: &Block) -> bool {
    body.iter().any(|s| match s.kind() {
        StmtKind::Return(_) => true,
        StmtKind::If(_, y, n) => has_return(&y) || has_return(&n),
        StmtKind::Match(_, arms) => arms.iter().any(|arm|has_return(&arm.body())),
        StmtKind::While(_, b) | StmtKind::For(_, _, _, b) => has_return(&b),
        _ => false,
    })
}
fn propagates(body: &Block) -> bool {
    fn expr(e: &Expr) -> bool {
        match e.kind() {
            ExprKind::Call(name, args) => name == "try" || args.iter().any(|argument|expr(&argument)),
            ExprKind::Invoke(callee, args) => expr(&callee) || args.iter().any(|argument|expr(&argument)),
            ExprKind::Specialize(_, _, args)
            | ExprKind::Cooperate(_, _, args)
            | ExprKind::Construct(_, args) => args.iter().any(|argument|expr(&argument)),
            ExprKind::Target(_, e) | ExprKind::Member(e, _) | ExprKind::Unary(_, e) => expr(&e),
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => expr(&a) || expr(&b),
            ExprKind::Select(c, a, b) => expr(&c) || expr(&a) || expr(&b),
            _ => false,
        }
    }
    body.iter().any(|s| match s.kind() {
        StmtKind::Let(_, _, e)
        | StmtKind::Expr(e)
        | StmtKind::Return(Some(e))
        | StmtKind::Seek(_, e) => expr(&e),
        StmtKind::Assign(a, _, b) => expr(&a) || expr(&b),
        StmtKind::If(c, a, b) => {
            expr(&c) || propagates(&a) || propagates(&b)
        }
        StmtKind::Match(value, arms) => expr(&value) || arms.iter().any(|arm|propagates(&arm.body())),
        StmtKind::While(c, body) => expr(&c) || propagates(&body),
        StmtKind::For(_, a, b, body) => expr(&a) || b.as_ref().is_some_and(expr) || propagates(&body),
        StmtKind::Parallel(_, domain, body) => {
            domain.count().as_ref().is_some_and(expr)
                || domain.selection().as_ref().is_some_and(expr)
                || propagates(&body)
        }
        _ => false,
    })
}
fn owns_exit(code: &[Inst]) -> bool {
    code.iter().any(|i| match &(i).kind {
        InstKind::Exit => true,
        InstKind::If { yes, no, .. } => owns_exit(yes) || owns_exit(no),
        InstKind::Loop {
            condition, body, ..
        } => owns_exit(condition) || owns_exit(body),
        _ => false,
    })
}
impl Builder<'_> {
    pub(super) fn written_model(&mut self,name:&str,span:Span)->Result<Returned> {
        match self.env.get(name).cloned().unwrap() {
            Binding::Owned(owner)=>{
                if !self.is_initialized(&owner.place)&&!self.defer_initialization(&owner.place,"B229",span)?{return self.fail(span,"B229",format!("返回前没有完整初始化独占参数 {name}"));}
                let regs=owner.place.ty().components.iter().map(|(scalar,_)|self.reg(*scalar)).collect();
                Ok(Returned::value(Value{ty:owner.place.ty().clone(),regs,proof:owner.proof,uniform:owner.uniform,literal:false,constant:None}))
            }
            Binding::OwnedValue(value)=>{
                let mut model=Returned::OwnedValue(value);self.resolve_owned_model(&mut model,None);self.prepare_owned_read(&model);let Returned::OwnedValue(mut value)=model else{unreachable!()};
                if !value.all_available(){return self.fail(span,"B229",format!("返回前没有归还独占参数 {name} 的全部活动拥有资源"));}
                let place=value.place().unwrap();
                if !value.value.ty.has_variants() && !self.is_initialized(&place)&&!self.defer_initialization(&place,"B229",span)?{return self.fail(span,"B229",format!("返回前没有完整初始化独占参数 {name}"));}
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
        initialized: &mut [address::InitializationEffect],
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
        for effect in initialized {*effect=effect.output_bound(&bindings);}
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
            Some(Returned::Value(value)) => Ok(Some(*value)),
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
        let value = match (expected.as_ref(), expr) {
            (Expected::Value(ty), Some(expr)) => {
                let result = self.evaluate_returned(expr,Some(ty))?;
                if !self.flow.continues() {
                    return Ok(());
                }
                let mut value = result.ok_or_else(|| Diagnostic {
                    source: self.scope.source().clone(),
                    span,
                    code: "B228",
                    message: "返回需要值或拥有资源".into(),
                })?;
                self.check_returned_type(&mut value, ty, expr.span())?;
                Some(value)
            }
            (Expected::Dataset { ty, count }, Some(expr)) => {
                let returned=self.evaluate_returned(expr,Some(&Ty::owned_dataset(ty.clone().syntax(),*count)))?;
                if !self.flow.continues(){return Ok(());}
                let Some(Returned::Dataset(mut data))=returned else{return self.fail(span,"B224","返回需要实际拥有的dataset")};
                self.application_data_type(&mut data, ty, *count, span)?;
                data.count = *count;
                Some(Returned::Dataset(data))
            }
            (Expected::Void, None) => None,
            (Expected::Never, expr) => {
            if let Some(expr) = expr {
                self.evaluate_returned(expr, None)?;
                if !self.flow.continues() {
                    return Ok(());
                }
            }
            return self.fail(span, "B228", "never 函数不能正常返回");
        },
            (Expected::Value(_) | Expected::Dataset { .. }, None) => return self.fail(span, "B228", "有返回值函数不能使用 return;"),
            (Expected::Void, Some(_)) => return self.fail(span, "B228", "void 函数不能返回普通值"),
        };
        self.finish_return(value, span)
    }
    pub(super) fn result_return_type(&self) -> Option<Ty> {
        match self.return_context.as_ref()?.expected.as_ref() {
            Expected::Value(ty) => Some(ty.clone()),
            _ => None,
        }
    }
    fn dataset_output_position(position:&Position,data:&application::Data)->Position {
        fn projected(data:&application::Data,index:Option<bounds::Operand>,word:u32)->Position {
            let layout=address::Layout::of(&data.ty);
            let origin=address::Origin{
                root:address::Root::Dataset{borrow:view::Borrow::source(view::Source::Parameter(data.root.identity(),Vec::new())),index},
                base:layout.clone(),layout:layout.clone(),words:layout.parts.iter().map(|(_,word)|*word).collect(),
                positions:Arc::from([]),writable:true,initialized:vec![false;layout.parts.len()].into(),
            };
            Position::Borrowed(Arc::new(view::Borrow::source(view::Source::Position(Arc::new(origin)))),layout,word)
        }
        let Some(Position::Parameter(scope,_))=&data.initialization else{return position.clone();};
        match position {
            Position::Parameter(owner,word)if owner==scope=>projected(data,None,*word as u32),
            Position::ApplicationElement(owner,index,word)if owner==scope=>{
                let index=match index {ownership::ApplicationIndex::Constant(value)=>bounds::Operand::Constant(Scalar::U32,Bits::from(*value)),ownership::ApplicationIndex::Value(id,_)=>bounds::Operand::Value(*id)};
                projected(data,Some(index),*word)
            }
            Position::Both(a,b)=>Position::both(Self::dataset_output_position(a,data),Self::dataset_output_position(b,data)),
            Position::Conditional(when,a,b)=>Position::selected(when.as_ref().clone(),Self::dataset_output_position(a,data),Self::dataset_output_position(b,data)),
            Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|position|Self::dataset_output_position(position,data)).collect(),*index),
            _=>position.clone(),
        }
    }
    pub(super) fn defer_dataset_output(&mut self,data:&application::Data,positions:Vec<Position>,span:Span)->Result<bool> {
        if data.access!=Access::Write||self.scope.execution()!=crate::execution::Requirement::Invocation {
            return self.defer_position_initialization(positions,"B229",span);
        }
        // The callee retains its own writes until the caller supplies the actual loan.
        let read=address::InitializationRead{
            positions:positions.iter().map(|position|Self::dataset_output_position(position,data)).collect(),
            initialized:self.initialized.iter().map(|position|Self::dataset_output_position(position,data)).collect(),
        };
        self.check_view_requirement(view::Requirement::Initialized{borrow:read.borrow(),read,code:"B229",span})?;
        Ok(true)
    }
    pub(super) fn finish_return(&mut self, mut value: Option<Returned>, span: Span) -> Result<()> {
        if let Some(value)=&mut value {self.require_returned_initialized(value,span)?;}
        if let Some(Returned::Value(value)) = &mut value {
            value.uniform &= self.control_uniform;
            value.proof = bounds::Proof::returning(value, &self.env, &self.path);
        }
        if let Some(Returned::OwnedValue(value)) = &mut value {
            value.value.proof = bounds::Proof::returning(&value.value, &self.env, &self.path);
        }
        let borrowed=(||->Result<BTreeMap<String,Returned>> {
            let mut borrowed=BTreeMap::new();let instance=self.return_context.as_ref().unwrap().instance.clone();let definition=&instance.definition;
            let inputs=self.return_context.as_ref().unwrap().inputs.clone();
            let returned=value.as_ref().map(|value|self.returned_storage(value));
            for (index,param) in definition.function.params().iter().enumerate() {
                if param.ty().dataset() && matches!(param.access(),Access::Write|Access::InOut) && let Some(Binding::ApplicationData(data))=self.env.get(param.name()).cloned() {
                    if outputs::declared(&instance,index) {
                        let guards=outputs::guards(self,&instance,index,&data.ty,&inputs,returned.as_ref())?;
                        self.check_data_output(&data,&guards,span)?;
                    }else if !self.application_initialized(&data)
                        && !self.defer_dataset_output(&data,data.ty.components.iter().map(|(_,word)|data.initialization.as_ref().unwrap().application_word(None,*word)).collect(),span)? {
                        return self.fail(span,"B229",format!("返回前没有完整初始化独占 dataset 参数 {}",param.name()));
                    }
                    borrowed.insert(param.name().to_owned(),Returned::Dataset(data));
                }
                if param.ty().dataset()&&param.access()==Access::Write&&let Some(Binding::Data(view))=self.env.get(param.name()).cloned() {
                    let place=self.dataset_element(&view,self.global_index,false);
                    let guards=outputs::guards(self,&instance,index,place.ty(),&inputs,returned.as_ref())?;
                    let path=self.path.clone();
                    for(position,when)in self.positions(&place).into_iter().zip(guards) {
                        self.path=when.condition().assume(&path,true);
                        if self.path.possible()&&!self.positions_initialized(std::slice::from_ref(&position))
                            &&!self.defer_position_initialization(vec![position],"B229",span)? {
                            return self.fail(span,"B229","返回前没有初始化本次dataset借用的输出分量");
                        }
                    }
                    self.path=path;
                }
                if matches!(param.access(),Access::Write|Access::InOut) && !param.ty().dataset() && !param.ty().tile() {
                    let mut model=if outputs::declared(&instance,index) {
                        let ty=self.env[param.name()].stored_place().unwrap().ty().clone();
                        let guards=outputs::guards(self,&instance,index,&ty,&inputs,returned.as_ref())?;
                        self.conditional_written_model(&param.name(),&guards,span)?
                    }else{self.written_model(&param.name(),span)?};
                    if let Some(value)=model.view_value_mut(){value.uniform&=self.control_uniform;value.proof.under_path(&value.ty,&self.path);}
                    borrowed.insert(param.name().to_owned(),model);
                }
            }
            Ok(borrowed)
        })();
        let mut owned=view::Borrow::default();let mut retained=view::Borrow::default();
        let outputs=value.iter().chain(borrowed.as_ref().ok().into_iter().flat_map(|written|written.values())).collect::<Vec<_>>();
        for output in &outputs{
            let resources=match *output{Returned::Dataset(data)=>vec![data],Returned::OwnedValue(value)=>value.resources(),_=>Vec::new()};
            for data in resources{owned.union(&self.owned_resource_borrow(data));}
        }
        for output in &outputs{self.require_element_return_borrows(output,&owned,span)?;}
        for output in outputs{if let Some(value)=output.view_value(){
            let mut proof=value.proof.borrow_state();proof.retain_view_borrows(&value.ty);
            self.require_view_return_proof(&proof,owned.clone(),span)?;
            retained.union(&proof.borrowed_view());
        }}
        // 返回取消外层求值；临时位置已没有后续消费者，拥有临时值仍由原释放点接管。
        self.env.retain(|name,binding| !name.starts_with('\0') || match binding {
            Binding::ApplicationData(data)=>data.ownership.may_own(),
            Binding::OwnedValue(value)=>value.ownership.may_own(),
            _=>false,
        });
        self.view_uses.returning();
        // 实际返回拥有者保持到同一次清理结束；只有正常完成才交给原ReturnExit。
        let pending=match &value {
            Some(Returned::OwnedValue(value))=>Some(self.hold_binding(Binding::OwnedValue(value.clone()))),
            Some(Returned::Dataset(data))=>Some(self.hold_binding(Binding::ApplicationData(data.clone()))),
            _=>None,
        };
        let keep=pending.iter().cloned().collect();
        self.release_application_data(Some(&keep),retained)?;
        if let Some(pending)=pending {self.env.remove(&pending);}
        let mut initialized=self.initialized.clone();
        if self.scope.execution()==crate::execution::Requirement::Invocation {
            for parameter in self.scope.parameters() {
                if parameter.bound(self.world)?.ty.dataset&&parameter.access()==Access::Write&&let Some(Binding::ApplicationData(data))=self.env.get(parameter.name()) {
                    initialized=initialized.iter().map(|position|Self::dataset_output_position(position,data)).collect();
                }
            }
        }
        let initialized=address::initialization_effects(&initialized,&self.path);
        let memory=self.view_model.uses.len();
        self.require_return_borrow(view::Borrow{reads:view::Edges::from([view_contracts::ContentRead::Returned{memory}]),..view::Borrow::default()},owned,span)?;
        let context = self.return_context.as_mut().unwrap();
        if context.scoped {
            if let Some(value) = &value {
                for (slot, reg) in context.slots.iter().zip(value.registers()) {
                    self.code.push(Inst { source_site: self.source_site.clone(), ..Inst::new(InstKind::StoreLocal(*slot, reg)) });
                }
            }
            self.code.push(Inst { source_site: self.source_site.clone(), ..Inst::new(InstKind::Exit) });
        }
        context.exits.push(ReturnExit {
            path: self.path.clone(),
            value,
            borrowed,
            initialized,
        });
        self.flow = Flow::RETURN;
        Ok(())
    }
    pub(super) fn function_body(&mut self, instance: &crate::generics::Instance, body: &Block) -> Result<(Option<Returned>,Vec<Option<Returned>>)> {
        let state = self.begin_function_body(instance, body)?;
        let mut sequence = if self.residency_path().possible() { Flow::NEXT } else { Flow::STOP };
        for statement in body {
            if !sequence.continues() {
                break;
            }
            self.flow = Flow::NEXT;
            self.statement(&statement)?;
            self.view_uses.after(statement.span());
            sequence = sequence.then(self.flow);
        }
        self.flow = sequence;
        self.finish_function_body(body, state)
    }

    #[inline(never)]
    fn begin_function_body(&mut self, instance: &crate::generics::Instance, body: &Block) -> Result<Box<BodyState>> {
        let entry = self.registers.len();
        let definition = &instance.definition;
        let name = instance.bindings.type_name(self.world, &definition.function.result(),
            (&definition.source, &definition.module, &definition.imports), definition.function.span())?;
        let expected = if name.name == "void" && !name.dataset && !name.tile && !name.view {
            Expected::Void
        } else if name.is_never() {
            Expected::Never
        } else {
            let ty = self.world.ty(
                &name,
                &definition.source,
                definition.function.span(),
                &definition.module,
                &definition.imports,
            )?;
            if ty.owns_resources()
                && (definition.function.kind() != FunctionKind::Value
                    || self.program.target != Target::Cpu)
            {
                return self.fail(
                    definition.function.span(),
                    "B224",
                    "拥有结果由普通CPU函数返回；并行元素与通用拥有聚合尚未实现",
                );
            }
            if name.dataset {
                Expected::Dataset {
                    ty,
                    count: self.world.shape(
                        &name.count,
                        &definition.source,
                        definition.function.span(),
                        &definition.module,
                        &definition.imports,
                    )?,
                }
            } else {
                Expected::Value(ty)
            }
        };
        let scoped = early_returns(body);
        if scoped && definition.function.kind() == FunctionKind::Cooperative {
            return self.fail(
                definition.function.span(),
                "B208",
                "cooperative 提前返回的整块收敛语义尚未实现；协作函数保持末尾返回",
            );
        }
        let slots = if scoped {
            expected
                .scalars()
                .into_iter()
                .map(|scalar| self.local_slots(&Ty::scalar(scalar)).0[0])
                .collect()
        } else {
            Vec::new()
        };
        let initial = self.application_initialization_state();
        let inputs=outputs::entry_inputs(self,instance)?;
        let caller = self.return_context.replace(ReturnContext {
            instance: instance.clone(),
            expected: Arc::new(expected),
            scoped,
            slots,
            exits: Vec::new(),
            inputs,
        });
        let flow = std::mem::replace(&mut self.flow, Flow::NEXT);
        let uniform = self.control_uniform;
        let code_start = self.code.len();
        Ok(Box::new(BodyState { entry, scoped, initial, caller, flow, uniform, code_start }))
    }

    #[inline(never)]
    fn finish_function_body(&mut self, body: &Block, state: Box<BodyState>) -> Result<(Option<Returned>,Vec<Option<Returned>>)> {
        let BodyState { entry, scoped, initial, caller, flow, uniform, code_start } = *state;
        let instance = self.return_context.as_ref().unwrap().instance.clone();
        let definition = &instance.definition;
        let sequence = self.flow;
        if matches!(self.return_context.as_ref().unwrap().expected.as_ref(), Expected::Never) && sequence.continues() {
            return self.fail(
                definition.function.span(),
                "B133",
                "never 函数存在可以正常结束的路径",
            );
        }
        if sequence.continues() {
            if matches!(self.return_context.as_ref().unwrap().expected.as_ref(), Expected::Value(_) | Expected::Dataset { .. }) {
                return self.fail(
                    definition.function.span(),
                    if body.is_empty() { "B132" } else { "B133" },
                    "函数存在未返回普通值的继续路径",
                );
            }
            self.return_statement(None, definition.function.span())?;
        }
        let context = self.return_context.take().unwrap();
        self.return_context = caller;
        self.flow = flow;
        self.control_uniform = uniform;
        let mut code = self.code.split_off(code_start);
        let single_exit = scoped
            && matches!(code.last().map(|inst| &inst.kind), Some(InstKind::Exit))
            && !owns_exit(&code[..code.len() - 1]);
        if single_exit {
            code.pop();
        }
        if scoped && !single_exit {
            self.emit(InstKind::Scope(code));
        } else {
            self.code.extend(code);
        }
        self.initialized = initial;
        let mut exits = context.exits.into_iter();
        let Some(first) = exits.next() else {
            self.flow = Flow::STOP;
            self.path = bounds::Clause::impossible();
            return Ok((None,vec![None;definition.function.params().len()]));
        };
        let mut path = first.path;
        let mut result = first.value;
        let mut borrowed = first.borrowed?;
        let mut initialized=first.initialized;
        let registers: Vec<_> = if scoped {
            context
                .slots
                .iter()
                .zip(context.expected.scalars())
                .map(|(slot, scalar)| {
                    let reg = self.reg(scalar);
                    self.emit(InstKind::LoadLocal(reg, *slot));
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
            &mut initialized,
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
            let mut other_initialized=exit.initialized;
            self.bind_return_exit(
                &mut other,
                &mut other_borrowed,
                &mut other_initialized,
                &mut path,
                &registers,
                &written,
                entry,
            );
            initialized.extend(other_initialized);
            let choice=bounds::Condition::paths(self.path.clone(),path.clone());
            self.path = bounds::Clause::either(&self.path, &path);
            if let (Some(value), Some(other)) = (&mut result, other) {
                value.join(other,false,Some(&choice));
            }
            for (name, owner) in &mut borrowed {
                let other = &other_borrowed[name];
                owner.join(other.clone(),false,Some(&choice));
            }
        }
        // 出口已分别检查并合流；不能再用最后解析分支的环境重建输出。
        for(name,model)in &mut borrowed{native_function::rebind_model(model,written[name].clone());}
        let output_models=definition.function.params().iter().map(|parameter|borrowed.get(parameter.name()).cloned()).collect();
        let returned=result.as_ref().map(|value|{let mut value=self.returned_storage(value);value.regs=registers.clone();value});
        for(name,model)in borrowed {
            let binding=match model {
                Returned::Dataset(data)=>Binding::ApplicationData(data),
                mut model=>{
                    let index=definition.function.params().iter().position(|parameter|parameter.name()==name).unwrap();
                    let place=self.env[&name].stored_place().unwrap();
                    let guards=outputs::guards(self,&instance,index,place.ty(),&context.inputs,returned.as_ref())?;
                    if outputs::declared(&instance,index) && place.ty().owns_resources(){outputs::apply_ownership(&mut model,place.ty(),0,&guards);}
                    match model {
                        Returned::Value(value)=>{let Some(Binding::Owned(mut owner))=self.env.get(&name).cloned()else{unreachable!()};owner.proof=value.proof;owner.uniform=value.uniform;self.initialize_output(&owner.place,&guards);Binding::Owned(owner)}
                        Returned::OwnedValue(mut value)=>{self.initialize_output(&place,&guards);value.place=Some(place);Binding::OwnedValue(value)}
                        _=>unreachable!(),
                    }
                },
            };
            self.env.insert(name,binding);
        }
        if scoped {
            if let Some(value) = &mut result {
                self.rebind_returned(value, registers);
            }
        }
        if self.return_context.is_none(){self.view_model.initialization=initialized;self.view_model.normal_returns=Some(bounds::Condition::execution(&self.path));}
        Ok((result,output_models))
    }
}

crate::module::codec::variants!(Returned{0=>Unit,1=>Value(value),2=>Dataset(data),3=>OwnedValue(value)});
