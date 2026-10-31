//! 顺序块合成真实退出边；循环结束只合流条件失败与 break 的状态。
use super::*;
use super::view::{Borrow,InputSlot,LoopInput,data_paths,data_at,data_at_mut};
impl Place {
    pub(super) fn join_position_range(&mut self, other: &Self) {
        match (self, other) {
            (
                Self::ApplicationElement { index_range: a, .. },
                Self::ApplicationElement { index_range: b, .. },
            ) => {
                if a.0 > a.1 {
                    *a = *b;
                } else if b.0 <= b.1 {
                    a.0 = a.0.min(b.0);
                    a.1 = a.1.max(b.1);
                }
            }
            (Self::Indexed { base: a, .. }, Self::Indexed { base: b, .. }) => {
                a.join_position_range(b);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Flow(u8);
impl Flow {
    pub const STOP: Self = Self(0);
    pub const NEXT: Self = Self(1);
    pub const RETURN: Self = Self(2);
    pub const BREAK: Self = Self(4);
    pub const CONTINUE: Self = Self(8);
    pub fn continues(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn iterates(self) -> bool {
        self.0 & (1 | 8) != 0
    }
    pub fn then(self, next: Self) -> Self {
        Self((self.0 & !1) | next.0)
    }
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub fn loop_result(self, next: bool) -> Self {
        Self((self.0 & 2) | u8::from(next))
    }
}
#[derive(Clone)]
pub(super) struct LoopState {
    env: BTreeMap<String, Binding>,
    path: bounds::Clause,
    initialized: BTreeSet<Position>,
    control: bool,
    cursor: Option<Value>,
}
pub(super) struct LoopContext {
    names: BTreeSet<String>,
    resources: BTreeMap<String, Binding>,
    exits: Vec<LoopState>,
    initial: bool,
    initialization: bool,
    continues: Vec<LoopState>,
    iterator: Option<String>,
    initial_cursor: Option<Value>,
    controls: BTreeMap<Reg,Value>,
    pub(super) view_inputs: Vec<view::InputSlot>,
    pub(super) view_resources: BTreeMap<ValueIdentity,view::Borrow>,
    pub(super) view_scope: Option<usize>,
}
impl Builder<'_> {
    pub(super) fn prepare_view_loop(&mut self) {
        if self.application.is_none() || self.loop_contexts.last().unwrap().initial{return;}
        let level=self.loop_contexts.len()-1;
        let scope=self.view_model.scope();
        self.loop_contexts[level].view_scope=Some(scope);
        let mut slots=Vec::new();
        for (name,binding) in &self.loop_contexts[level].resources {
            for (path,_,independent) in data_paths(binding){slots.push(InputSlot::Data{name:name.clone(),path,independent});}
            if let Some((ty,_))=binding.view_parts() {
                for component in view_contracts::components(ty) {
                    slots.push(InputSlot::Value{name:name.clone(),component});
                }
            }
        }
        for (index,slot)in slots.iter().enumerate(){if slot.independent(){self.view_model.independent.insert(LoopInput{level:scope,index});}}
        let mut resources:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Data{name,path,..}=slot {
                let input=Borrow{inputs:view::Edges::from([LoopInput{level:scope,index}]),..Borrow::default()};
                for environment in [&self.env,&self.loop_contexts[level].resources] {
                    if let Some(data)=environment.get(name).and_then(|b|data_at(b,path)) {resources.entry(data.handle.identity()).or_default().union(&input);}
                }
            }
        }
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Data{name,path,..}=slot {
                let data=data_at_mut(self.env.get_mut(name).unwrap(),path).unwrap();
                data.handle.proof=data.handle.proof.clone().with_view_borrow(0,Borrow{inputs:view::Edges::from([LoopInput{level:scope,index}]),..Borrow::default()});
            }
        }
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Value{name,component}=slot {
                let proof=self.env.get_mut(name).unwrap().view_proof_mut().unwrap();
                *proof=proof.clone().with_view_borrow(*component,Borrow{inputs:view::Edges::from([LoopInput{level:scope,index}]),..Borrow::default()});
            }
        }
        self.loop_contexts[level].view_inputs=slots;self.loop_contexts[level].view_resources=resources;
    }
    pub(super) fn close_view_context(&mut self,context:&mut LoopContext,natural:Option<usize>,nonempty:bool)->Result<()> {
        let Some(scope)=context.view_scope else{return Ok(());};
        let mut initial=Vec::new();
        for slot in &context.view_inputs {
            initial.push(match slot {
                InputSlot::Value{name,component}=>context.resources[name].view_parts().unwrap().1.view_borrow(*component),
                InputSlot::Data{name,path,..}=>self.data_value_borrow(data_at(&context.resources[name],path).unwrap()),
            });
        }
        let data_value=view::resource_borrow;
        let mut definitions=initial.clone();
        let mut continued=vec![Borrow::default();definitions.len()];
        for state in &context.continues {
            for (index,slot) in context.view_inputs.iter().enumerate() {
                let value=match slot {
                    InputSlot::Value{name,component}=>state.env.get(name).and_then(Binding::view_parts).map(|(_,proof)|proof.view_borrow(*component)).unwrap_or_default(),
                    InputSlot::Data{name,path,..}=>state.env.get(name).and_then(|b|data_at(b,path)).map(&data_value).unwrap_or_default(),
                };
                definitions[index].union(&value);continued[index].union(&value);
            }
        }
        if nonempty {
            if let Some(natural)=natural {
                for slot in &context.view_inputs {
                    if let InputSlot::Value{name,component}=slot {
                        let proof=context.exits[natural].env.get_mut(name).unwrap().view_proof_mut().unwrap();
                        let mut borrow=proof.view_borrow(*component);
                        let inputs=std::mem::take(&mut borrow.inputs);
                        for(input,when)in inputs.into_entries(){if input.level==scope{borrow.union(&continued[input.index].restricted(&when));}else{borrow.inputs.insert_when(input,when);}}
                        *proof=proof.clone().with_view_borrow(*component,borrow);
                    }
                }
            }
        }
        for (index,definition)in definitions.into_iter().enumerate(){self.view_model.loops.insert(LoopInput{level:scope,index},definition);}
        // 每个出口保留自己实际交还的资源，再用同一入口关系消去局部循环引用。
        let resolve_env=|environment:&mut BTreeMap<String,Binding>| {
            for binding in environment.values_mut() {
                let mut data_borrows=data_paths(binding).into_iter().map(|(path,data,_)|(path,data_value(data))).collect::<BTreeMap<_,_>>();
                match binding {
                    Binding::ApplicationData(data)=>{if let Some(value)=data_borrows.remove(&Vec::new()){data.handle.proof=data.handle.proof.clone().with_view_borrow(0,value);}}
                    Binding::OwnedValue(value)=>{
                        fn assign(value:&mut owned_values::OwnedValue,path:&mut Vec<usize>,borrows:&mut BTreeMap<Vec<usize>,Borrow>) {
                            for (index,payload) in value.payloads.iter_mut().enumerate(){let Some(payload)=payload.as_deref_mut()else{continue};path.push(index);
                                match payload {returns::Returned::Dataset(data)=>{if let Some(value)=borrows.remove(path){data.handle.proof=data.handle.proof.clone().with_view_borrow(0,value);}},returns::Returned::OwnedValue(value)=>assign(value,path,borrows),_=>{}}
                                path.pop();
                            }
                        }
                        assign(value,&mut Vec::new(),&mut data_borrows);
                    }
                    _=>{},
                }
            }
        };
        for state in &mut context.exits {resolve_env(&mut state.env);}
        resolve_env(&mut self.env);
        Ok(())
    }
    pub(super) fn finish_view_condition(&mut self)->Result<()> {
        let mut context=self.loop_contexts.pop().unwrap();self.close_view_context(&mut context,None,false)
    }

    pub(super) fn initialize_iteration_exit(
        &self,
        exit: &mut LoopState,
        outputs: &[(application::Data, Vec<u32>)],
        index: ownership::ApplicationIndex,
        initial: bool,
    ) {
        let continues = &self.loop_contexts.last().unwrap().continues;
        for (data, words) in outputs {
            let scope = data.initialization.as_ref().unwrap();
            for word in words {
                if initial
                    || continues.iter().all(|state| {
                        initialized_application_word(&state.initialized, scope, index, *word)
                    })
                {
                    exit.initialized
                        .insert(scope.application_word(None,*word));
                }
            }
        }
    }
    pub(super) fn initialize_array_iteration_exit(
        &self,
        exit: &mut LoopState,
        begin: &Value,
        limit: &Value,
        index: ownership::ApplicationIndex,
        initial: bool,
        body: &[Stmt],
    ) {
        if initial || bounds::seeks(body) {
            return;
        }
        let states = &self.loop_contexts.last().unwrap().continues;
        if states.is_empty() {
            return;
        }
        let positions: BTreeSet<_> = states
            .iter()
            .flat_map(|state| state.initialized.iter().cloned())
            .collect();
        for position in positions {
            if !states.iter().all(|state| {
                self.position_initialized_in(&position, &state.initialized, &state.env)
            }) {
                continue;
            }
            if begin.maximum() < limit.minimum() && !position.uses_index(index) {
                exit.initialized.insert(position.clone());
            }
            if begin.minimum() == begin.maximum()
                && limit.minimum() == limit.maximum()
                && let Position::Indexed(choices, at) = &position
                && *at == index
                && let Some(covered) =
                    choices.get(begin.minimum() as usize..limit.minimum() as usize)
            {
                exit.initialized.extend(covered.iter().cloned());
            }
        }
    }
    fn initial_target(&self,expr:&Expr,shape:bool)->Option<bool>{
        use returns::Returned;use application::Ownership;
        if let ExprKind::Name(name)=&expr.kind{if let Some(Binding::ApplicationData(data))=self.env.get(name){return Some(!self.resolve_ownership(&data.ownership,None).available());}}
        if !self.owned_element_type(expr)?.owns_resources(){return Some(false);}
        let projection=self.owned_projection(expr)?;let Binding::OwnedValue(root)=&self.env[&projection.root]else{unreachable!()};let mut model=Returned::OwnedValue(root.clone());
        for index in projection.path{let Returned::OwnedValue(value)=model else{unreachable!()};model=value.payloads[index].as_deref().unwrap().clone();}
        self.resolve_owned_model(&mut model,None);Some(match model{Returned::Dataset(data)=>!data.ownership.available(),Returned::OwnedValue(value)=>if value.value.ty.has_variants(){value.ownership==Ownership::Moved||matches!(value.ownership,Ownership::Conditional(_))||shape&&value.incomplete()}else{!value.available()},_=>false})
    }
    pub(super) fn application_initial_loop(&self,body:&[Stmt])->bool{
        fn exits(body:&[Stmt],nested:bool)->bool{body.iter().any(|statement|match &statement.kind{
            StmtKind::Break=>!nested,StmtKind::Return(_)=>true,
            StmtKind::If(_,yes,no)=>exits(yes,nested)||exits(no,nested),
            StmtKind::Match(_,arms)=>arms.iter().any(|arm|exits(&arm.body,nested)),
            StmtKind::While(_,body)|StmtKind::For(_,_,_,body)=>exits(body,true),
            _=>false,
        })}
        // 资源出口先消费真实首轮；后续回边仍沿原构造进展及位置关系处理。
        if self.application.is_some()&&self.env.values().any(|binding|!binding.application_resources().is_empty())&&exits(body,false){return true;}
        fn root(expr:&Expr)->Option<&str>{match &expr.kind{ExprKind::Name(name)=>Some(name),ExprKind::Member(base,_)|ExprKind::Index(base,_)=>root(base),_=>None}}
        effects::value_writes(self.world,&self.definition,body).iter().any(|expr|match self.initial_target(expr,true){Some(value)=>value,None=>match root(expr).and_then(|name|self.env.get(name)){Some(Binding::OwnedValue(value))=>value.ownership==application::Ownership::Moved||value.incomplete(),Some(Binding::ApplicationData(data))=>data.ownership==application::Ownership::Moved,_=>false}})
    }
    pub(super) fn next_initial_loop(&self,body:&[Stmt])->bool{
        if !self.application_initial_loop(body){return false;}
        if effects::value_writes(self.world,&self.definition,body).iter().any(|expr|self.initial_target(expr,false)==Some(true)){return true;}
        #[derive(PartialEq,Eq,PartialOrd,Ord)]enum Known{Owned(Vec<usize>),Payload(Vec<usize>),Boolean(usize,bool)}
        fn collect(model:&returns::Returned,path:&mut Vec<usize>,values:&mut BTreeSet<Known>){match model{returns::Returned::Dataset(data)=>{if data.ownership==application::Ownership::Owned{values.insert(Known::Owned(path.clone()));}},returns::Returned::OwnedValue(value)=>{if value.value.ty.has_variants()&&value.ownership==application::Ownership::Owned{values.insert(Known::Owned(path.clone()));}for(index,payload)in value.payloads.iter().enumerate(){if let Some(payload)=payload{path.push(index);if value.value.ty.has_variants(){values.insert(Known::Payload(path.clone()));}collect(payload,path,values);path.pop();}}},_=>{}}}
        let context=self.loop_contexts.last().unwrap();let known=|environment:&BTreeMap<String,Binding>|{let mut all=BTreeSet::new();for name in context.resources.keys(){
            let scalar=match environment.get(name){Some(Binding::Value(value))=>Some((&value.ty,bounds::Proof::capture(value))),Some(Binding::Owned(value))=>Some((value.place.ty(),value.proof.clone())),Some(Binding::OwnedValue(value))=>Some((&value.value.ty,bounds::Proof::capture(&value.value))),_=>None};
            if let Some((ty,proof))=scalar{for(component,(kind,_))in ty.components.iter().enumerate(){if *kind==Scalar::Bool && let Some(value)=proof.project(&[component],ty.components.len()).known_value(&Ty::scalar(Scalar::Bool)){all.insert((name.clone(),Known::Boolean(component,value[0]!=0)));}}}
            let model=match environment.get(name){Some(Binding::ApplicationData(data))=>Some(returns::Returned::Dataset(data.clone())),Some(Binding::OwnedValue(value))=>Some(returns::Returned::OwnedValue(value.clone())),_=>None};if let Some(mut model)=model{self.resolve_owned_model(&mut model,None);let mut values=BTreeSet::new();collect(&model,&mut Vec::new(),&mut values);all.extend(values.into_iter().map(|value|(name.clone(),value)));}}all};let before=known(&context.resources);let after=known(&self.env);before.is_subset(&after)&&before!=after
    }
    pub(super) fn configure_initial_loop(
        &mut self,
        initial: bool,
        iterator: Option<String>,
        cursor: Option<Value>,
    ) {
        let initialization = iterator.is_some() && self.incomplete_positions();
        let context = self.loop_contexts.last_mut().unwrap();
        context.initialization = initialization;
        context.initial = initial;
        context.iterator = iterator;
        context.initial_cursor = cursor;
    }
    pub(super) fn initial_loop_cursor(&self) -> Option<Value> {
        self.loop_contexts
            .last()
            .and_then(|context| context.initial_cursor.clone())
    }
    pub(super) fn capture_loop_cursor(&mut self, value: Value) {
        if let Some(name) = self
            .loop_contexts
            .last()
            .and_then(|context| context.iterator.clone())
        {
            self.env.insert(name, Binding::Value(value));
        }
    }
    pub(super) fn check_application_loop(&mut self, span: Span) -> Result<()> {
        if self.loop_contexts.last().unwrap().initial {
            let state = self.loop_state();
            self.loop_contexts.last_mut().unwrap().continues.push(state);
            return Ok(());
        }
        let path=self.residency_path();
        let entry = &self.loop_contexts.last().unwrap().resources;
        for (name, binding) in &self.env {
            if let Binding::OwnedValue(result) = binding
                && let Some(Binding::OwnedValue(before)) = entry.get(name)
            {
                if result.ownership != before.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的结果所有权，不能在回边重复消费",
                    );
                }
                if result.ownership == application::Ownership::Owned
                    && !before.loop_compatible(
                        result,
                        &mut self.residency,
                        &self.definition.source,
                        span,
                        &path,
                    )
                {
                    return self.fail(span,"B266","循环回边尚未能合流结果载荷的分支与CPU／GPU存储位置；需要相同的资源生命周期");
                }
            }
            if let Binding::ApplicationData(data) = binding
                && let Some(Binding::ApplicationData(before)) = entry.get(name)
            {
                if before.ownership != data.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的 dataset 所有权，不能在回边重复消费",
                    );
                }
                if before.ownership != application::Ownership::Moved
                    && !self.residency.same(
                        &before.residency,
                        &data.residency,
                        &self.definition.source,
                        span,
                        &path,
                    )
                {
                    return self.fail(
                        span,
                        "B266",
                        "循环继续前须归还入口相同的 CPU／GPU 存储位置；在循环内完成配对的显式传输",
                    );
                }
            }
        }
        if !self.loop_contexts.last().unwrap().view_inputs.is_empty() || self.loop_contexts.last().unwrap().initialization || self.env.values().any(|binding| binding.application_resources().iter().any(|data|data.initialization.is_some())) {
            let state = self.loop_state();
            self.loop_contexts.last_mut().unwrap().continues.push(state);
        }
        Ok(())
    }
    pub(super) fn release_application_loop(&mut self) -> Result<()> {
        let names = self.loop_contexts.last().unwrap().names.clone();
        self.release_application_data(Some(&names),view::Borrow::default())
    }
    pub(super) fn remember_loop_control(&mut self,value:&Value){if let Some(context)=self.loop_contexts.last_mut(){context.controls.insert(value.regs[0],value.clone());}}
    pub(super) fn enter_loop(&mut self, environment: &BTreeMap<String, Binding>) {
        self.loop_contexts.push(LoopContext {
            names: environment.keys().cloned().collect(),
            resources: environment.clone(),
            exits: Vec::new(),
            initial: false,
            initialization: false,
            continues: Vec::new(),
            iterator: None,
            initial_cursor: None,
            controls: BTreeMap::new(),
            view_inputs: Vec::new(),view_resources: BTreeMap::new(),view_scope:None,
        });
    }
    pub(super) fn loop_state(&self) -> LoopState {
        let names = &self.loop_contexts.last().unwrap().names;
        LoopState {
            path: self.path.clone(),
            env: self
                .env
                .iter()
                .filter(|(n, _)| names.contains(*n))
                .map(|(n, b)| (n.clone(), b.clone()))
                .collect(),
            initialized: self.application_initialization_state(),
            control: self.control_uniform,
            cursor: self
                .loop_contexts
                .last()
                .unwrap()
                .iterator
                .as_ref()
                .and_then(|name| match self.env.get(name) {
                    Some(Binding::Value(value)) => Some(value.clone()),
                    _ => None,
                }),
        }
    }
    fn restore_initial_edge(&mut self, state: LoopState) {
        self.env = state.env;
        if let(Some(name),Some(cursor))=(&self.loop_contexts.last().unwrap().iterator,state.cursor){self.env.insert(name.clone(),Binding::Value(cursor));}
        self.path = state.path;
        self.initialized = state.initialized;
        self.control_uniform = state.control;
    }
    pub(super) fn expand_initial_loop<F>(
        &mut self,
        code: Vec<Inst>,
        next: bool,
        mut continuation: F,
    ) -> Result<(Vec<Inst>, Flow)>
    where
        F: FnMut(&mut Self, Option<Value>) -> Result<()>,
    {
        let context = self.loop_contexts.last_mut().unwrap();
        let breaks = std::mem::take(&mut context.exits);
        let mut continues = std::mem::take(&mut context.continues);
        let natural = if next {
            Some(continues.pop().unwrap())
        } else {
            None
        };
        let mut exits = Vec::new();
        let mut flow = Flow::NEXT;
        let mut code = expand_initial_code(
            self,
            code,
            &mut breaks.into_iter(),
            &mut continues.into_iter(),
            &mut exits,
            &mut flow,
            &mut continuation,
        )?;
        if let Some(state) = natural {
            code.extend(compile_initial_continuation(
                self,
                state,
                &mut exits,
                &mut flow,
                &mut continuation,
            )?);
        }
        self.loop_contexts.last_mut().unwrap().exits = exits;
        Ok((code, flow))
    }
    pub(super) fn capture_break(&mut self) {
        let state = self.loop_state();
        self.loop_contexts.last_mut().unwrap().exits.push(state);
    }
    pub(super) fn leave_loop(
        &mut self,
        mut natural:Option<LoopState>,
        test:&Value,
        body:&[Inst],
        span:Span,
        nonempty:bool,
    )->Result<bool>{
        let mut context=self.loop_contexts.pop().unwrap();let natural_index=context.exits.len();
        if let Some(state)=natural.take(){context.exits.push(state);}
        let view_natural=(context.exits.len()>natural_index).then_some(natural_index);self.close_view_context(&mut context,view_natural,nonempty)?;
        natural=if context.exits.len()>natural_index{Some(context.exits.pop().unwrap())}else{None};
        let mut guards=loop_exit_paths(body,&context.controls,&owned_presence::Presence::input(test));
        if let Some(state)=natural{context.exits.push(state);guards.push(owned_presence::Presence::input(test).not());}
        let initialized=context.exits.iter().zip(&guards).flat_map(|(state,when)|state.initialized.iter().map(move|position|Position::selected(when.clone(),position.clone(),Position::Initialized))).collect::<BTreeSet<_>>();
        let common=context.exits.iter().map(|state|state.initialized.clone()).reduce(|a,b|ownership::intersect_initialized(&a,&b));
        let mut guards=guards.into_iter();let mut exits=context.exits.into_iter();let Some(first)=exits.next()else{self.env.retain(|name,_|context.names.contains(name));self.path=bounds::Clause::impossible();return Ok(false)};
        let _first=guards.next().unwrap();self.env=first.env;self.path=first.path;
        for exit in exits{let when=guards.next().unwrap();let mut previous=std::mem::replace(&mut self.env,exit.env);self.path=bounds::Clause::either(&self.path,&exit.path);self.join_environment_selected(&mut previous,&when);}
        self.initialized=common.unwrap();self.initialized.extend(initialized);self.view_uses.after(span);Ok(true)
    }
}
// 每个出口条件来自原Loop/If；不写运行时出口标记，也不丢弃其他路径的拥有内容。
fn loop_exit_paths(code:&[Inst],controls:&BTreeMap<Reg,Value>,test:&owned_presence::Presence)->Vec<owned_presence::Presence>{
    use owned_presence::Presence;
    fn escapes(code:&[Inst],exit:bool)->bool{code.iter().any(|instruction|match instruction{Inst::Break|Inst::Continue=>true,Inst::Exit=>exit,Inst::If{yes,no,..}=>escapes(yes,exit)||escapes(no,exit),Inst::Scope(code)=>escapes(code,false),_=>false})}
    fn walk(code:&[Inst],mut when:Presence,controls:&BTreeMap<Reg,Value>,breaks:&mut Vec<Presence>)->(Presence,Presence){
        let mut exited=Presence::None;
        for instruction in code{match instruction{
            Inst::Break=>{breaks.push(when.clone());when=Presence::None;},Inst::Continue=>when=Presence::None,Inst::Exit=>{exited=exited.or(when);when=Presence::None;},
            Inst::If{condition,yes,no}if escapes(yes,true)||escapes(no,true)=>{let selected=Presence::input(&controls[condition]);let(a,x)=walk(yes,when.clone().and(selected.clone()),controls,breaks);let(b,y)=walk(no,when.and(selected.not()),controls,breaks);when=a.or(b);exited=exited.or(x).or(y);},
            Inst::Scope(code)if escapes(code,false)=>{let(a,b)=walk(code,when,controls,breaks);when=a.or(b);},_=>{}
        }}(when,exited)
    }
    let mut breaks=Vec::new();walk(code,Presence::All,controls,&mut breaks);breaks.into_iter().map(|when|test.clone().and(when)).collect()
}

// Continue is the real next-iteration edge.// Continue is the real next-iteration edge. Compile its continuation directly, without an ownership flag.
fn compile_initial_continuation<'a, F>(
    builder: &mut Builder<'a>,
    state: LoopState,
    exits: &mut Vec<LoopState>,
    flow: &mut Flow,
    continuation: &mut F,
) -> Result<Vec<Inst>>
where
    F: FnMut(&mut Builder<'a>, Option<Value>) -> Result<()>,
{
    let cursor = state.cursor.clone();
    builder.restore_initial_edge(state);
    builder.flow = Flow::NEXT;
    continuation(builder, cursor)?;
    *flow = flow.union(builder.flow);
    let mut code = std::mem::take(&mut builder.code);
    if builder.flow.continues() {
        exits.push(builder.loop_state());
        code.push(Inst::Break);
    }
    Ok(code)
}
fn expand_initial_code<'a, F>(
    builder: &mut Builder<'a>,
    code: Vec<Inst>,
    breaks: &mut impl Iterator<Item = LoopState>,
    continues: &mut impl Iterator<Item = LoopState>,
    exits: &mut Vec<LoopState>,
    flow: &mut Flow,
    continuation: &mut F,
) -> Result<Vec<Inst>>
where
    F: FnMut(&mut Builder<'a>, Option<Value>) -> Result<()>,
{
    let mut result = Vec::new();
    for mut instruction in code {
        match &mut instruction {
            Inst::Break => exits.push(breaks.next().unwrap()),
            Inst::Continue => {
                result.extend(compile_initial_continuation(
                    builder,
                    continues.next().unwrap(),
                    exits,
                    flow,
                    continuation,
                )?);
                continue;
            }
            Inst::If { yes, no, .. } => {
                *yes = expand_initial_code(
                    builder,
                    std::mem::take(yes),
                    breaks,
                    continues,
                    exits,
                    flow,
                    continuation,
                )?;
                *no = expand_initial_code(
                    builder,
                    std::mem::take(no),
                    breaks,
                    continues,
                    exits,
                    flow,
                    continuation,
                )?;
            }
            Inst::Scope(body) => {
                *body = expand_initial_code(
                    builder,
                    std::mem::take(body),
                    breaks,
                    continues,
                    exits,
                    flow,
                    continuation,
                )?
            }
            _ => {}
        }
        result.push(instruction);
    }
    Ok(result)
}

pub(super) fn initialized_application_word(
    initialized: &BTreeSet<Position>,
    scope: &Position,
    index: ownership::ApplicationIndex,
    word: u32,
) -> bool {
    scope.application_word(Some(index),word).initialized_by(initialized)
}
