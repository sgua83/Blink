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
#[derive(Clone,Default)]
pub(super) struct LoopState {
    pub(super) env: BTreeMap<String, Binding>,
    pub(super) path: bounds::Clause,
    pub(super) initialized: BTreeSet<Position>,
    control: bool,
    pub(super) cursor: Option<Value>,
}
pub(super) struct LoopContext {
    pub outer: Vec<Inst>,
    pub writes: effects::LoopWrites,
    names: BTreeSet<String>,
    pub(super) entry: LoopState,
    exits: Vec<LoopState>,
    initial: bool,
    initialization: bool,
    continues: Vec<LoopState>,
    iterator: Option<(String, Value)>,
    pub(super) control_source: view_contracts::LoopControl,
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
        for (name,binding) in &self.loop_contexts[level].entry.env {
            for (path,data,independent) in data_paths(binding){
                slots.push(InputSlot::Data{name:name.clone(),path:path.clone(),independent});
                if let Some(value)=data.elements.view_value(){for component in view_contracts::components(&value.ty){
                    slots.push(InputSlot::Value{name:name.clone(),contents:Some(path.clone()),component,identity:None});
                }}
            }
            if let Some((ty,_))=binding.view_parts() {
                for component in view_contracts::components(ty) {
                    slots.push(InputSlot::Value{name:name.clone(),contents:None,component,identity:None});
                }
            }
        }
        for (index,slot)in slots.iter().enumerate(){if slot.independent(){self.view_model.independent.insert(LoopInput{level:scope,index});}}
        let mut resources:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Data{name,path,..}=slot {
                let input=Borrow{inputs:view::Edges::from([LoopInput{level:scope,index}]),..Borrow::default()};
                for environment in [&self.env,&self.loop_contexts[level].entry.env] {
                    if let Some(data)=environment.get(name).and_then(|b|data_at(b,path)) {resources.entry(data.root.identity()).or_default().union(&input);}
                }
            }
        }
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Data{name,path,..}=slot {
                let data=data_at_mut(self.env.get_mut(name).unwrap(),path).unwrap();
                let input=LoopInput{level:scope,index};
                let before=data_at(&self.loop_contexts[level].entry.env[name],path).unwrap();
                if !data.ty.zero_valid() && (data.root.proof.known_identity(0)!=before.root.proof.known_identity(0)
                    || data.root.proof.view_borrow(0)!=before.root.proof.view_borrow(0)) {
                    data.initialization=Some(Position::ApplicationInput(input,None,0));
                }
                data.root.proof=data.root.proof.clone().with_view_borrow(0,Borrow{inputs:view::Edges::from([input]),..Borrow::default()});
            }
        }
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Value{component,..}=slot {
                let proof=slot.value_proof_mut(&mut self.env).unwrap();
                *proof=proof.clone().with_view_borrow(*component,Borrow{inputs:view::Edges::from([LoopInput{level:scope,index}]),..Borrow::default()});
            }
        }
        self.loop_contexts[level].view_inputs=slots;self.loop_contexts[level].view_resources=resources;
    }
    pub(super) fn close_view_context(&mut self,context:&mut LoopContext,natural:Option<usize>,nonempty:bool)->Result<()> {
        let Some(scope)=context.view_scope else{return Ok(());};
        let mut sources=Vec::new();
        for slot in &context.view_inputs {
            sources.push(match slot {
                InputSlot::Value{component,..}=>slot.value_proof(&context.entry.env).unwrap().view_borrow(*component),
                InputSlot::Data{name,path,..}=>self.data_value_borrow(data_at(&context.entry.env[name],path).unwrap()),
            });
        }
        let data_value=view::resource_borrow;
        let mut definitions=sources.clone();
        let mut continued=vec![Borrow::default();definitions.len()];
        for state in &context.continues {
            for (index,slot) in context.view_inputs.iter().enumerate() {
                let value=match slot {
                    InputSlot::Value{component,..}=>slot.value_proof(&state.env).map(|proof|proof.view_borrow(*component)).unwrap_or_default(),
                    InputSlot::Data{name,path,..}=>state.env.get(name).and_then(|b|data_at(b,path)).map(&data_value).unwrap_or_default(),
                };
                definitions[index].union(&value);continued[index].union(&value);
            }
        }
        if nonempty {
            if let Some(natural)=natural {
                for slot in &context.view_inputs {
                    if let InputSlot::Value{component,..}=slot {
                        let proof=slot.value_proof_mut(&mut context.exits[natural].env).unwrap();
                        let mut borrow=proof.view_borrow(*component);
                        let inputs=std::mem::take(&mut borrow.inputs);
                        for(input,when)in inputs.into_entries(){if input.level==scope{borrow.union(&continued[input.index].restricted(&when));}else{borrow.inputs.insert_when(input,when);}}
                        *proof=proof.clone().with_view_borrow(*component,borrow);
                    }
                }
            }
        }
        // 直接自身边不向循环来源的最小不动点贡献新来源。
        let mut direct=vec![false;sources.len()];
        for (index,definition)in definitions.iter_mut().enumerate(){
            definition.inputs.remove(&LoopInput{level:scope,index});
            direct[index]=*definition==sources[index];
        }
        let resolve=|mut borrow:Borrow,direct:&[bool],sources:&[Borrow]| {
            let inputs=std::mem::take(&mut borrow.inputs);
            for(input,when)in inputs.into_entries(){
                if input.level==scope&&direct[input.index]{borrow.union(&sources[input.index].restricted(&when));}
                else{borrow.inputs.insert_when(input,when);}
            }
            borrow
        };
        for slot in &context.view_inputs {
            if let InputSlot::Value{name,component,identity:Some((id,_)),..}=slot {
                context.control_source.inputs.insert(*id,view_contracts::ControlInput::Value{name:name.clone(),component:*component});
            }
        }
        let control=Arc::new(std::mem::take(&mut context.control_source));
        let mut producers=std::mem::take(&mut context.continues);
        producers.insert(0,std::mem::take(&mut context.entry));
        let producers:Arc<[LoopState]>=producers.into();
        for (index,definition)in definitions.into_iter().enumerate(){
            let input=LoopInput{level:scope,index};let definition=resolve(definition,&direct,&sources);
            direct[index]=definition==sources[index];
            if matches!(context.view_inputs[index],InputSlot::Value{..})&&!definition.pending(){sources[index]=definition.clone();direct[index]=true;}
            let resource=match &context.view_inputs[index] {
                InputSlot::Data{name,path,..}=>Some(view_contracts::LoopResource{name:name.clone(),path:path.clone()}),
                _=>None,
            };
            let value=match &context.view_inputs[index]{InputSlot::Value{identity,..}=>*identity,_=>None};
            self.view_model.loops.insert(input,view_contracts::LoopDefinition{borrow:definition,resource,value,states:producers.clone(),control:control.clone()});
        }
        // 来源定义未改变的槽交回原入口；实际来源及存储版本变化保留原循环关系。
        let resolve_env=|environment:&mut BTreeMap<String,Binding>| {
            for slot in &context.view_inputs {
                if let InputSlot::Value{component,..}=slot && let Some(proof)=slot.value_proof_mut(environment){
                    let borrow=resolve(proof.view_borrow(*component),&direct,&sources);
                    *proof=std::mem::take(proof).with_view_borrow(*component,borrow);
                }
            }
            for binding in environment.values_mut() {
                let mut data_borrows=data_paths(binding).into_iter().map(|(path,data,_)|(path,resolve(data_value(data),&direct,&sources))).collect::<BTreeMap<_,_>>();
                match binding {
                    Binding::ApplicationData(data)=>{if let Some(value)=data_borrows.remove(&Vec::new()){data.root.proof=data.root.proof.clone().with_view_borrow(0,value);}}
                    Binding::OwnedValue(value)=>{
                        fn assign(value:&mut owned_values::OwnedValue,path:&mut Vec<usize>,borrows:&mut BTreeMap<Vec<usize>,Borrow>) {
                            for (index,payload) in value.payloads.iter_mut().enumerate(){let Some(payload)=payload.as_deref_mut()else{continue};path.push(index);
                                match payload {returns::Returned::Dataset(data)=>{if let Some(value)=borrows.remove(path){data.root.proof=data.root.proof.clone().with_view_borrow(0,value);}},returns::Returned::OwnedValue(value)=>assign(value,path,borrows),_=>{}}
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

    pub(super) fn application_iteration_elements(&mut self, exit: &mut LoopState, data: &application::DataReference) {
        let context = self.loop_contexts.last().unwrap();
        let (name, _) = context.iterator.as_ref().unwrap();
        let ty = data.element_layout().1;
        let mut common: Option<bounds::Proof> = None;
        for state in &context.continues {
            let Some(Binding::Owned(owner)) = state.env.get(name) else { return; };
            if let Some(proof) = &mut common { proof.join_condition(&owner.proof, ty.components.len(), None); }
            else { common = Some(owner.proof.clone()); }
        }
        let Some(mut proof) = common else { return; };
        self.application_iteration_interference(data, context.control_source.cursor.as_ref().unwrap(), &mut proof, &context.continues, &context.controls, &context.writes);
        self.publish_iteration_elements(exit, data, proof);
    }
    pub(super) fn application_indexed_elements(&mut self, exit: &mut LoopState, index: &Value, limit: &Value) {
        let context = self.loop_contexts.last().unwrap();
        let mut results = Vec::new();
        for binding in context.entry.env.values() {
            for data in binding.application_resources() {
                if data.root.ty == Ty::scalar(Scalar::U32)
                    && bounds::Condition::comparison(Comparison::Equal, &data.length, limit).truth_in(&context.entry.path) == Some(true) {
                    let proof = self.application_indexed_proof(data, index, &context.continues, &context.controls, &context.writes);
                    results.push((application::DataReference::direct(data.clone()), proof));
                }
            }
        }
        for (data, proof) in results { self.publish_iteration_elements(exit, &data, proof); }
    }
    fn publish_iteration_elements(&mut self, exit: &mut LoopState, data: &application::DataReference, proof: bounds::Proof) {
        let ty = data.element_layout().1;
        std::mem::swap(&mut self.env, &mut exit.env);
        std::mem::swap(&mut self.path, &mut exit.path);
        let mut proof = proof.at_call(&self.element_owner_bindings(&data.data));
        for (component, (scalar, _)) in ty.components.iter().enumerate() {
            if *scalar == Scalar::U32 && let Some(identity) = proof.known_identity(component) {
                proof.mark_domain(component, IndexDomain::Upper(identity, 0));
            }
        }
        let ranges = proof.element_ranges(0, ty);
        let components = ty.components.iter().enumerate().filter_map(|(component, (_, word))| {
            (ranges.iter().any(|range| range.offset == *word) || !proof.domains(component).is_empty()).then_some(component)
        }).collect::<Vec<_>>();
        if !components.is_empty() {
            let words = data.words();
            let words = components.iter().map(|component| words[*component]).collect::<Vec<_>>();
            self.update_application_contents(data, &words, application::ElementUpdate::WholeFacts(&proof.project(&components, ty.components.len())));
        }
        std::mem::swap(&mut self.env, &mut exit.env);
        std::mem::swap(&mut self.path, &mut exit.path);
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
            exit.initialized.extend(array_iteration_position(
                &position,index,
                (begin.minimum()==begin.maximum()&&limit.minimum()==limit.maximum()).then(||(begin.minimum(),limit.minimum())),
                begin.maximum()<limit.minimum(),
            ));
        }
    }

    pub(super) fn defer_iteration_exit(
        &self,exit:&mut LoopState,begin:&Value,limit:&Value,outputs:&[(application::Data,Vec<u32>)],
        index:ownership::ApplicationIndex,initial:bool,body:&[Stmt],
    ) {
        if initial||bounds::seeks(body){return;}
        let states=&self.loop_contexts.last().unwrap().continues;
        if !states.iter().any(|state|address::initialization_effects(&state.initialized,&bounds::Clause::default()).iter().any(address::InitializationEffect::pending)){return;}
        let iteration=address::Iteration{
            states:states.iter().map(|state|address::InitializationState{initialized:state.initialized.clone(),path:bounds::Condition::execution(&state.path)}).collect(),
            index,
            range:(begin.minimum()==begin.maximum()&&limit.minimum()==limit.maximum()).then(||(begin.minimum(),limit.minimum())),
            nonempty:begin.maximum()<limit.minimum(),
            outputs:outputs.iter().map(|(data,words)|(data.initialization.as_ref().unwrap().clone(),words.clone())).collect(),
        };
        exit.initialized.insert(Position::Initialization(Arc::new(address::InitializationEffect::Iteration{
            iteration:Arc::new(iteration),when:bounds::Condition::constant(true),
        })));
    }

    fn initial_target(&self,expr:&Expr,shape:bool)->Option<bool>{
        use returns::Returned;use application::Ownership;
        if let ExprKind::Name(name)=&expr.kind{if let Some(Binding::ApplicationData(data))=self.env.get(name){return Some(!self.resolve_ownership(&data.ownership,None).available());}}
        if !self.owned_element_type(expr)?.owns_resources(){return Some(false);}
        let projection=self.owned_projection(expr)?;let Binding::OwnedValue(root)=&self.env[&projection.root]else{unreachable!()};let mut model=Returned::OwnedValue(root.clone());
        for index in projection.path{let Returned::OwnedValue(value)=model else{unreachable!()};model=value.payloads[index].as_deref().unwrap().clone();}
        self.resolve_owned_model(&mut model,None);Some(match model{Returned::Dataset(data)=>!data.ownership.available(),Returned::OwnedValue(value)=>if value.value.ty.has_variants(){value.ownership==Ownership::Moved||matches!(value.ownership,Ownership::Conditional(_))||shape&&value.incomplete()}else{!value.available()},_=>false})
    }
    pub(super) fn application_initial_loop(&self,body:&[Stmt])->Result<bool>{
        fn exits(body:&[Stmt],nested:bool)->bool{body.iter().any(|statement|match &statement.kind{
            StmtKind::Break=>!nested,StmtKind::Return(_)=>true,
            StmtKind::If(_,yes,no)=>exits(yes,nested)||exits(no,nested),
            StmtKind::Match(_,arms)=>arms.iter().any(|arm|exits(&arm.body,nested)),
            StmtKind::While(_,body)|StmtKind::For(_,_,_,body)=>exits(body,true),
            _=>false,
        })}
        // 资源出口先消费真实首轮；后续回边仍沿原构造进展及位置关系处理。
        if self.application.is_some()&&self.env.values().any(|binding|!binding.application_resources().is_empty())&&exits(body,false){return Ok(true);}
        fn root(expr:&Expr)->Option<&str>{match &expr.kind{ExprKind::Name(name)=>Some(name),ExprKind::Member(base,_)|ExprKind::Index(base,_)=>root(base),_=>None}}
        Ok(effects::value_writes(self,body)?.iter().any(|expr|match self.initial_target(expr,true){Some(value)=>value,None=>match root(expr).and_then(|name|self.env.get(name)){Some(Binding::OwnedValue(value))=>value.ownership==application::Ownership::Moved||value.incomplete(),Some(Binding::ApplicationData(data))=>data.ownership==application::Ownership::Moved,_=>false}}))
    }
    pub(super) fn next_initial_loop(&self,body:&[Stmt])->Result<bool>{
        if !self.application_initial_loop(body)?{return Ok(false);}
        if effects::value_writes(self,body)?.iter().any(|expr|self.initial_target(expr,false)==Some(true)){return Ok(true);}
        #[derive(PartialEq,Eq,PartialOrd,Ord)]enum Known{Owned(Vec<usize>),Payload(Vec<usize>),Boolean(usize,bool)}
        fn collect(model:&returns::Returned,path:&mut Vec<usize>,values:&mut BTreeSet<Known>){match model{returns::Returned::Dataset(data)=>{if data.ownership==application::Ownership::Owned{values.insert(Known::Owned(path.clone()));}},returns::Returned::OwnedValue(value)=>{if value.value.ty.has_variants()&&value.ownership==application::Ownership::Owned{values.insert(Known::Owned(path.clone()));}for(index,payload)in value.payloads.iter().enumerate(){if let Some(payload)=payload{path.push(index);if value.value.ty.has_variants(){values.insert(Known::Payload(path.clone()));}collect(payload,path,values);path.pop();}}},_=>{}}}
        let context=self.loop_contexts.last().unwrap();let known=|environment:&BTreeMap<String,Binding>|{let mut all=BTreeSet::new();for name in context.entry.env.keys(){
            let scalar=match environment.get(name){Some(Binding::Value(value))=>Some((&value.ty,bounds::Proof::capture(value))),Some(Binding::Owned(value))=>Some((value.place.ty(),value.proof.clone())),Some(Binding::OwnedValue(value))=>Some((&value.value.ty,bounds::Proof::capture(&value.value))),_=>None};
            if let Some((ty,proof))=scalar{for(component,(kind,_))in ty.components.iter().enumerate(){if *kind==Scalar::Bool && let Some(value)=proof.project(&[component],ty.components.len()).known_value(&Ty::scalar(Scalar::Bool)){all.insert((name.clone(),Known::Boolean(component,value[0]!=0)));}}}
            let model=match environment.get(name){Some(Binding::ApplicationData(data))=>Some(returns::Returned::Dataset(data.clone())),Some(Binding::OwnedValue(value))=>Some(returns::Returned::OwnedValue(value.clone())),_=>None};if let Some(mut model)=model{self.resolve_owned_model(&mut model,None);let mut values=BTreeSet::new();collect(&model,&mut Vec::new(),&mut values);all.extend(values.into_iter().map(|value|(name.clone(),value)));}}all};let before=known(&context.entry.env);let after=known(&self.env);Ok(before.is_subset(&after)&&before!=after)
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
        context.iterator = iterator.zip(cursor.clone());
        context.entry.cursor=cursor.clone();
        context.control_source.cursor=cursor;
    }
    pub(super) fn loop_cursor(&self) -> Option<Value> {
        self.loop_contexts
            .last()
            .and_then(|context| context.control_source.cursor.clone())
    }
    pub(super) fn capture_loop_cursor(&mut self, value: Value) {
        if let Some((name,cursor))=self.loop_contexts.last_mut().and_then(|context|context.iterator.as_mut()) {
            *cursor=value.clone();
            if let Some(Binding::Value(source))=self.env.get_mut(name){*source=value;}
        }
    }
    pub(super) fn check_application_loop(&mut self, span: Span) -> Result<()> {
        if self.loop_contexts.last().unwrap().initial {
            let state = self.loop_state();
            self.loop_contexts.last_mut().unwrap().continues.push(state);
            return Ok(());
        }
        let path=self.residency_path();
        let entry = &self.loop_contexts.last().unwrap().entry.env;
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
        let state = self.loop_state();
        self.loop_contexts.last_mut().unwrap().continues.push(state);
        Ok(())
    }
    pub(super) fn release_application_loop(&mut self) -> Result<()> {
        let names = self.loop_contexts.last().unwrap().names.clone();
        self.release_application_data(Some(&names),view::Borrow::default())
    }
    pub(super) fn remember_loop_control(&mut self,value:&Value){
        if let Some(context)=self.loop_contexts.last_mut(){context.controls.insert(value.regs[0],value.clone());}
        let condition=bounds::Condition::of(value);
        let lengths=self.env.values().flat_map(view::data_paths).filter_map(|(_,data,_)|{
            let Some(Position::ApplicationInput(input,..))=&data.initialization else{return None;};
            let id=data.length.identity();condition.mentions(id).then_some((*input,id))
        }).collect::<Vec<_>>();
        for(input,id)in lengths {
            if let Some(context)=self.loop_contexts.iter_mut().find(|context|context.view_scope==Some(input.level))
                && let InputSlot::Data{name,path,..}=&context.view_inputs[input.index] {
                context.control_source.inputs.insert(id,view_contracts::ControlInput::Length{name:name.clone(),path:path.clone()});
            }
        }
    }
    pub(super) fn enter_loop(&mut self, environment: &BTreeMap<String, Binding>) {
        // The saved entry still has its real length. Complete storage no longer
        // depends on that length's precision after loop writes are projected.
        let complete=environment.iter().flat_map(|(name,binding)|data_paths(binding).into_iter()
            .filter(|(_,data,_)|self.application_initialized(data))
            .map(move|(path,_,_)|(name.clone(),path))).collect::<Vec<_>>();
        let mut resources=environment.clone();
        for(name,path)in complete {
            data_at_mut(resources.get_mut(&name).unwrap(),&path).unwrap().initialization=None;
            data_at_mut(self.env.get_mut(&name).unwrap(),&path).unwrap().initialization=None;
        }
        self.loop_contexts.push(LoopContext {
            outer: Vec::new(),writes: effects::LoopWrites::default(),
            names: environment.keys().cloned().collect(),
            entry:LoopState{env:resources,path:self.path.clone(),initialized:self.application_initialization_state(),control:self.control_uniform,cursor:None},
            exits: Vec::new(),
            initial: false,
            initialization: false,
            continues: Vec::new(),
            iterator: None,
            control_source:view_contracts::LoopControl::default(),
            controls: BTreeMap::new(),
            view_inputs: Vec::new(),view_resources: BTreeMap::new(),view_scope:None,
        });
    }
    pub(super) fn loop_state(&self) -> LoopState {
        let context=self.loop_contexts.last().unwrap();
        let names=&context.names;
        let iterator=context.iterator.as_ref().map(|(name,_)|name);
        LoopState {
            path: self.path.clone(),
            env: self
                .env
                .iter()
                .filter(|(n, _)| names.contains(*n)||iterator==Some(*n))
                .map(|(n, b)| (n.clone(), b.clone()))
                .collect(),
            initialized: self.application_initialization_state(),
            control: self.control_uniform,
            cursor: context.iterator.as_ref().map(|(_,cursor)|cursor.clone()),
        }
    }
    fn restore_initial_edge(&mut self, state: LoopState) {
        self.env = state.env;
        if let(Some((_,current)),Some(cursor))=(self.loop_contexts.last_mut().unwrap().iterator.as_mut(),state.cursor){*current=cursor;}
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
    )->Result<(bool,effects::LoopWrites)>{
        let mut context=self.loop_contexts.pop().unwrap();let natural_index=context.exits.len();
        if !context.initial && let Some(exit)=natural.as_mut() {
            let empty=context.control_source.test.as_ref().unwrap().at_call(&context.control_source.bindings(&context.entry)).negate();
            for(name,binding)in &mut exit.env {
                let Some((proof,uniform,place))=binding.proof_storage() else{continue;};
                let Some((entry,_,_))=context.entry.env.get_mut(name).and_then(Binding::proof_storage) else{continue;};
                let width=place.ty().components.len();
                for component in 0..width {
                    if !proof.known_identity(component).zip(entry.known_identity(component)).is_some_and(|(a,b)|a==b){continue;}
                    let mut repeated:Option<(bounds::Proof,bounds::Clause,Reg)>=None;
                    for edge in &mut context.continues {
                        if let Some((next,next_uniform,_))=edge.env.get_mut(name).and_then(Binding::proof_storage) {
                            let next=next.project(&[component],width);*uniform &= *next_uniform;
                            if let Some((common,aliases,reg))=repeated.as_mut(){
                                *aliases=bounds::Clause::either(aliases,&next.output_aliases(&[*reg],&self.registers,&BTreeMap::new()));
                                common.join(&next,1,None);
                            }else{
                                let reg=self.reg(place.ty().components[component].0);
                                let aliases=next.output_aliases(&[reg],&self.registers,&BTreeMap::new());
                                repeated=Some((next,aliases,reg));
                            }
                        }
                    }
                    if let Some((repeated,alias,reg))=repeated {
                        let mut common=proof.project(&[component],width);
                        let aliases=[common.output_aliases(&[reg],&self.registers,&BTreeMap::new()),alias];
                        exit.path=bounds::Clause::both(&exit.path,&bounds::Clause::either(&empty.assume(&aliases[0],true),&empty.assume(&aliases[1],false)));
                        common.join_condition(&repeated,1,Some(&empty));common=common.with_identity(ValueIdentity::Register(reg)).with_view_borrow(0,proof.view_borrow(component));
                        proof.assign(&[component],width,common);
                    }
                }
            }
        }
        if let Some(state)=natural.take(){context.exits.push(state);}
        let view_natural=(context.exits.len()>natural_index).then_some(natural_index);self.close_view_context(&mut context,view_natural,nonempty)?;
        natural=if context.exits.len()>natural_index{Some(context.exits.pop().unwrap())}else{None};
        let mut guards=loop_exit_paths(body,&context.controls,&owned_presence::Presence::input(test));
        if let Some(state)=natural{context.exits.push(state);guards.push(owned_presence::Presence::input(test).not());}
        let common=context.exits.iter().map(|state|state.initialized.clone()).reduce(|a,b|ownership::intersect_initialized(&a,&b));
        let initialized=context.exits.iter().zip(&guards).flat_map(|(state,when)|state.initialized.difference(common.as_ref().unwrap()).map(move|position|Position::selected(when.clone(),position.clone(),Position::Initialized))).collect::<BTreeSet<_>>();
        let mut guards=guards.into_iter();let mut exits=context.exits.into_iter();let Some(first)=exits.next()else{self.env.retain(|name,_|context.names.contains(name));self.path=bounds::Clause::impossible();return Ok((false,context.writes))};
        let _first=guards.next().unwrap();self.env=first.env;self.path=first.path;
        for exit in exits{let when=guards.next().unwrap();let mut previous=std::mem::replace(&mut self.env,exit.env);self.path=bounds::Clause::either(&self.path,&exit.path);self.join_environment_selected(&mut previous,&when);}
        self.env.retain(|name,_|context.names.contains(name));
        self.initialized=common.unwrap();self.initialized.extend(initialized);self.view_uses.after(span);Ok((true,context.writes))
    }
}
// 每个出口条件来自原Loop/If；不写运行时出口标记，也不丢弃其他路径的拥有内容。
fn loop_exit_paths(code:&[Inst],controls:&BTreeMap<Reg,Value>,test:&owned_presence::Presence)->Vec<owned_presence::Presence>{
    use owned_presence::Presence;
    fn escapes(code:&[Inst],exit:bool)->bool{code.iter().any(|instruction|match &(instruction).kind{InstKind::Break|InstKind::Continue=>true,InstKind::Exit=>exit,InstKind::If{yes,no,..}=>escapes(yes,exit)||escapes(no,exit),InstKind::Scope(code)=>escapes(code,false),_=>false})}
    fn walk(code:&[Inst],mut when:Presence,controls:&BTreeMap<Reg,Value>,breaks:&mut Vec<Presence>)->(Presence,Presence){
        let mut exited=Presence::None;
        for instruction in code{match &(instruction).kind{
            InstKind::Break=>{breaks.push(when.clone());when=Presence::None;},InstKind::Continue=>when=Presence::None,InstKind::Exit=>{exited=exited.or(when);when=Presence::None;},
            InstKind::If{condition,yes,no}if escapes(yes,true)||escapes(no,true)=>{let selected=Presence::input(&controls[condition]);let(a,x)=walk(yes,when.clone().and(selected.clone()),controls,breaks);let(b,y)=walk(no,when.and(selected.not()),controls,breaks);when=a.or(b);exited=exited.or(x).or(y);},
            InstKind::Scope(code)if escapes(code,false)=>{let(a,b)=walk(code,when,controls,breaks);when=a.or(b);},_=>{}
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
        code.push(Inst::new(InstKind::Break));
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
        match &mut (instruction).kind {
            InstKind::Break => exits.push(breaks.next().unwrap()),
            InstKind::Continue => {
                result.extend(compile_initial_continuation(
                    builder,
                    continues.next().unwrap(),
                    exits,
                    flow,
                    continuation,
                )?);
                continue;
            }
            InstKind::If { yes, no, .. } => {
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
            InstKind::Scope(body) => {
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


fn array_iteration_position(position:&Position,index:ownership::ApplicationIndex,range:Option<(u32,u32)>,nonempty:bool)->Vec<Position> {
    let mut result=Vec::new();
    if nonempty&&!position.uses_index(index){result.push(position.clone());}
    if let Some((begin,limit))=range
        &&let Position::Indexed(choices,at)=position&&*at==index
        &&let Some(covered)=choices.get(begin as usize..limit as usize) {
        result.extend(covered.iter().cloned());
    }
    result
}
pub(super) fn iteration_positions(iteration:&address::Iteration,states:&[BTreeSet<Position>])->BTreeSet<Position> {
    let mut result=BTreeSet::new();
    for(scope,words)in &iteration.outputs {
        for word in words {
            if states.iter().all(|state|initialized_application_word(state,scope,iteration.index,*word)){
                result.insert(scope.application_word(None,*word));
            }
        }
    }
    let positions=states.iter().flat_map(|state|state.iter().cloned()).collect::<BTreeSet<_>>();
    for position in positions {
        if states.iter().all(|state|position.initialized_by(state)){
            result.extend(array_iteration_position(&position,iteration.index,iteration.range,iteration.nonempty));
        }
    }
    result
}

pub(super) fn initialized_application_word(
    initialized: &BTreeSet<Position>,
    scope: &Position,
    index: ownership::ApplicationIndex,
    word: u32,
) -> bool {
    scope.application_word(Some(index),word).initialized_by(initialized)
}
