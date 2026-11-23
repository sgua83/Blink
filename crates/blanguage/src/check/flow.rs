//! 顺序块合成真实退出边；循环结束只合流条件失败与 break 的状态。
use super::*;
use super::view::{Borrow,InputSlot,LoopInput,data_at,data_at_mut};
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
    pub fn then(self, next: Self) -> Self {
        Self((self.0 & !1) | next.0)
    }
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
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
    first_register: Reg,
    first_use: usize,
    pub outer: Vec<Inst>,
    pub writes: effects::LoopWrites,
    names: BTreeSet<String>,
    pub(super) entry: LoopState,
    exits: Vec<LoopState>,
    initial: bool,
    initialization: bool,
    continues: Vec<LoopState>,
    pub(super) iterator: Option<(String, Value)>,
    pub(super) control_source: view_contracts::LoopControl,
    controls: BTreeMap<Reg,Value>,
    pub(super) view_inputs: Vec<view::InputSlot>,
    pub(super) view_resources: BTreeMap<ValueIdentity,view::Borrow>,
    pub(super) view_scope: Option<usize>,
}
impl LoopContext {
    fn carries_binding(&self, name: &str) -> bool {
        self.names.contains(name) || self.iterator.as_ref().is_some_and(|(iterator, _)| iterator == name)
    }

    pub(super) fn value_input(&mut self,name:&str,contents:Option<Vec<usize>>,component:usize,scalar:Scalar)->ValueIdentity {
        let index=self.view_inputs.iter().position(|slot|matches!(slot,InputSlot::Value{name:target,contents:path,component:part,..}if target==name&&path==&contents&&*part==component)).unwrap_or(self.view_inputs.len());
        let identity=ValueIdentity::LoopValue(self.view_scope.unwrap(),index);
        let slot=InputSlot::Value{name:name.to_owned(),contents,component,identity:Some((identity,scalar))};
        if index==self.view_inputs.len(){self.view_inputs.push(slot);}else{self.view_inputs[index]=slot;}
        identity
    }
}
impl Builder<'_> {
    pub(super) fn prepare_view_loop(&mut self) {
        if self.application.is_none() || self.loop_contexts.last().unwrap().initial{return;}
        let level=self.loop_contexts.len()-1;
        let scope=self.view_model.scope();
        self.loop_contexts[level].view_scope=Some(scope);
        let mut slots=Vec::new();
        for (name,binding) in &self.loop_contexts[level].entry.env {
            view::for_each_data_path(binding, |path, data, independent| {
                slots.push(InputSlot::Data { name: name.clone(), path: path.to_vec(), independent });
                if let Some(value) = data.elements.known().and_then(returns::Returned::view_value) {
                    for component in view_contracts::components(&value.ty) {
                        slots.push(InputSlot::Value {
                            name: name.clone(), contents: Some(path.to_vec()), component, identity: None,
                        });
                    }
                }
            });
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
                if !data.ty.zero_valid() {
                    data.initialization=Some(Position::ApplicationInput(input,None,0));
                }
                data.root.proof.set_view_borrow(0,Borrow{inputs:view::Edges::from([input]),..Borrow::default()});
            }
        }
        for (index,slot) in slots.iter().enumerate() {
            if let InputSlot::Value{component,..}=slot {
                let proof=slot.value_proof_mut(&mut self.env).unwrap();
                proof.set_view_borrow(*component,Borrow{inputs:view::Edges::from([LoopInput{level:scope,index}]),..Borrow::default()});
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
        let mut definitions = sources.clone();
        let natural = if nonempty { natural } else { None };
        let mut continued = BTreeMap::new();
        if let Some(natural) = natural {
            let env = &context.exits[natural].env;
            for slot in &context.view_inputs {
                if let InputSlot::Value { component, .. } = slot {
                    let proof = slot.value_proof(env).unwrap();
                    for input in proof.view_borrow_ref(*component).into_iter().flat_map(|borrow| borrow.inputs.iter()) {
                        if input.level == scope {
                            continued.entry(input.index).or_insert_with(Borrow::default);
                        }
                    }
                }
            }
        }
        for state in &context.continues {
            for (index, slot) in context.view_inputs.iter().enumerate() {
                let value = match slot {
                    InputSlot::Value { component, .. } => slot.value_proof(&state.env).map(|proof| proof.view_borrow(*component)).unwrap_or_default(),
                    InputSlot::Data { name, path, .. } => state.env.get(name).and_then(|b| data_at(b, path)).map(&data_value).unwrap_or_default(),
                };
                definitions[index].union(&value);
                if let Some(continued) = continued.get_mut(&index) {
                    continued.union(&value);
                }
            }
        }
        if let Some(natural) = natural {
            for slot in &context.view_inputs {
                if let InputSlot::Value { component, .. } = slot {
                    let proof = slot.value_proof_mut(&mut context.exits[natural].env).unwrap();
                    let borrow = proof.view_borrow(*component)
                        .project_loop_inputs(scope, |index| Some(&continued[&index]));
                    proof.set_view_borrow(*component, borrow);
                }
            }
        }
        // 直接自身边不向循环来源的最小不动点贡献新来源。
        let mut direct=vec![false;sources.len()];
        for (index,definition)in definitions.iter_mut().enumerate(){
            definition.inputs.remove(&LoopInput{level:scope,index});
            direct[index]=*definition==sources[index];
        }
        let resolve = |borrow: Borrow, direct: &[bool], sources: &[Borrow]| {
            borrow.project_loop_inputs(scope, |input| direct[input].then(|| &sources[input]))
        };
        for slot in &context.view_inputs {
            if let InputSlot::Value{name,contents,component,identity:Some((id,_))}=slot {
                context.control_source.inputs.insert(*id,view_contracts::ControlInput::Value{name:name.clone(),contents:contents.clone(),component:*component});
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
                    proof.set_view_borrow(*component,borrow);
                }
            }
            let mut resolve_data = |data: &mut application::Data| {
                if data.ownership != application::Ownership::Moved {
                    let borrow = resolve(data_value(data), &direct, &sources);
                    data.root.proof.set_view_borrow(0, borrow);
                }
            };
            for binding in environment.values_mut() {
                match binding {
                    Binding::ApplicationData(data) => resolve_data(data),
                    Binding::OwnedValue(value) => value.resources_mut(&mut resolve_data),
                    _ => {}
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

    pub(super) async fn application_iteration_elements(&mut self, exit: &mut LoopState, source: Option<&application::DataReference>, index: &Value, limit: &Value)->Result<()> {
        let context = self.loop_contexts.last().unwrap();
        let resources = if let Some(data) = source { vec![data.clone()] } else {
            context.entry.env.values().flat_map(Binding::application_resources).filter(|data|
                data.root.ty == Ty::scalar(Scalar::U32)
                    && bounds::Condition::comparison(Comparison::Equal, &data.length, limit).truth_in(&context.entry.path) == Some(true)
            ).map(|data| application::DataReference::direct(data.clone())).collect()
        };
        for data in resources {
            let model=unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.scope.span())).await }?;
            let initial=self.returned_storage(&model);
            let context = self.loop_contexts.last().unwrap();
            let cursor = if source.is_some() { context.control_source.cursor.as_ref().unwrap() } else { index };
            let ty = data.element_layout().1;
            let components=data.words().iter().map(|word|data.ty.components.iter().position(|(_,at)|at==word).unwrap()).collect::<Vec<_>>();
            let complete=context.entry.cursor.as_ref().is_some_and(|begin|begin.minimum()==0&&begin.maximum()==0);
            let (normal,broken)=self.application_indexed_proof(&data.data,cursor,&initial,context.continues.iter().chain(context.exits.iter()),&context.controls,&context.writes);
            let normal=normal.project(&components,data.ty.components.len());
            let partial=(!complete||!context.exits.is_empty()).then(||{
                let mut proof=bounds::Proof::capture(&initial).project(&components,data.ty.components.len());
                if !context.continues.is_empty(){proof.join_condition(&normal,ty.components.len(),None);}
                if !context.exits.is_empty(){proof.join_condition(&broken.project(&components,data.ty.components.len()),ty.components.len(),None);}
                self.application_iteration_interference(&data,cursor,&mut proof,context.continues.iter().chain(context.exits.iter()),&context.controls,&context.writes);
                proof
            });
            let natural=if complete{
                let common=if source.is_some(){
                    let (name, _) = context.iterator.as_ref().unwrap();
                    let mut common: Option<bounds::Proof> = None;
                    for state in &context.continues {
                        let Some(Binding::Owned(owner)) = state.env.get(name) else { return Ok(()); };
                        let incoming=bounds::scoped_proof(ty,owner.proof.clone(),&state.path,&state.env);
                        if let Some(proof) = &mut common { proof.join_condition(&incoming, ty.components.len(), None); }
                        else { common = Some(incoming); }
                    }
                    common.map(|mut proof|{proof.constrain_common_ranges(&normal,ty.components.len());proof})
                }else{Some(normal)};
                common.map(|mut proof|{
                    self.application_iteration_interference(&data,cursor,&mut proof,context.continues.iter(),&context.controls,&context.writes);proof
                })
            }else{partial.clone()};
            let mut breaks=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().exits);
            if let Some(proof)=partial{for state in &mut breaks{unsafe { self.world.call(self.publish_iteration_elements(state,&data,proof.clone())).await }?;}}
            self.loop_contexts.last_mut().unwrap().exits=breaks;
            if let Some(proof)=natural{unsafe { self.world.call(self.publish_iteration_elements(exit,&data,proof)).await }?;}
        }
        Ok(())
    }
    async fn publish_iteration_elements(&mut self, exit: &mut LoopState, data: &application::DataReference, proof: bounds::Proof)->Result<()> {
        let ty = data.element_layout().1;
        std::mem::swap(&mut self.env, &mut exit.env);
        std::mem::swap(&mut self.path, &mut exit.path);
        let mut proof = proof.at_call(&self.element_owner_bindings(&data.data));
        for (component, (scalar, _)) in ty.components.iter().enumerate() {
            if let Some(identity) = proof.known_identity(component) {
                proof.mark_domain(component, IndexDomain::CommonRange(identity, *scalar, *scalar, *scalar, *scalar));
                if *scalar == Scalar::U32 { proof.mark_domain(component, IndexDomain::Upper(identity, 0)); }
            }
        }
        let ranges = proof.element_ranges(0, ty);
        let components = ty.components.iter().enumerate().filter_map(|(component, (_, word))| {
            (ranges.iter().any(|range| range.offset == *word) || !proof.domains(component).is_empty()
                || proof.component_condition(component).is_some_and(|condition| !condition.unconstrained())).then_some(component)
        }).collect::<Vec<_>>();
        if !components.is_empty() {
            let words = data.words();
            let words = components.iter().map(|component| words[*component]).collect::<Vec<_>>();
            unsafe { self.world.call(self.update_application_contents(data, &words, application::ElementUpdate::WholeFacts(&proof.project(&components, ty.components.len())))).await }?;
        }
        std::mem::swap(&mut self.env, &mut exit.env);
        std::mem::swap(&mut self.path, &mut exit.path);
        Ok(())
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
                        scope.application_word_initialized(Some(index), *word, &state.initialized)
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
        body: &Block,
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
                self.position_initialized_in(&position, &state.initialized, &state.env,None)
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
        index:ownership::ApplicationIndex,initial:bool,body:&Block,
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
        if let ExprKind::Name(name)=expr.kind(){if let Some(Binding::ApplicationData(data))=self.env.get(name){return Some(!self.resolve_ownership(&data.ownership,None).available());}}
        if !self.owned_element_type(expr)?.owns_resources(){return Some(false);}
        let projection=self.owned_projection(expr)?;let Binding::OwnedValue(root)=&self.env[&projection.root]else{unreachable!()};let mut model=Returned::OwnedValue(root.clone());
        for index in projection.path{let Returned::OwnedValue(value)=model else{unreachable!()};model=value.payloads[index].as_deref().unwrap().clone();}
        self.resolve_owned_model(&mut model,None);Some(match model{Returned::Dataset(data)=>!data.ownership.available(),Returned::OwnedValue(value)=>if value.value.ty.has_variants(){value.ownership==Ownership::Moved||matches!(value.ownership,Ownership::Conditional(_))||shape&&value.incomplete()}else{!value.available()},_=>false})
    }
    fn loop_has_resource_exit(&self, body: &Block) -> bool {
        if self.application.is_none()
            || !self.env.values().any(|binding| !binding.application_resources().is_empty())
        {
            return false;
        }
        let storage = body.source_view().storage();
        unsafe { source_loop_exit(storage.nodes.cast_const(), body.source_slice()) }
    }

    fn loop_writes_incomplete_resource(&self, writes: &[Expr]) -> bool {
        fn root(expr: &Expr) -> Option<String> {
            match expr.kind() {
                ExprKind::Name(name) => Some(name.into()),
                ExprKind::Member(base, _) | ExprKind::Index(base, _) => root(&base),
                _ => None,
            }
        }

        writes.iter().any(|expr| match self.initial_target(expr, true) {
            Some(value) => value,
            None => match root(expr).and_then(|name| self.env.get(&name)) {
                Some(Binding::OwnedValue(value)) => {
                    value.ownership == application::Ownership::Moved || value.incomplete()
                }
                Some(Binding::ApplicationData(data)) => data.ownership == application::Ownership::Moved,
                _ => false,
            },
        })
    }

    pub(super) async fn application_initial_loop(&self, body: &Block) -> Result<bool> {
        if self.loop_has_resource_exit(body) {
            return Ok(true);
        }
        let writes = unsafe { self.world.call(effects::value_writes(self, body)).await }?;
        Ok(self.loop_writes_incomplete_resource(&writes))
    }

    pub(super) async fn next_initial_loop(&self, body: &Block) -> Result<bool> {
        let resource_exit = self.loop_has_resource_exit(body);
        let writes = unsafe { self.world.call(effects::value_writes(self, body)).await }?;
        if !resource_exit && !self.loop_writes_incomplete_resource(&writes) {
            return Ok(false);
        }
        if writes.iter().any(|expr| self.initial_target(expr, false) == Some(true)) {
            return Ok(true);
        }
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
        self.loop_contexts.last().and_then(|context|context.iterator.as_ref()).map(|(name,cursor)|{
            match &self.env[name]{Binding::Value(value)=>value.clone(),_=>cursor.clone()}
        })
    }
    fn loop_compatibility_inputs(&self,cursor:Option<&Value>) -> (bounds::Clause, BTreeMap<ValueIdentity, bounds::Proof>) {
        let context = self.loop_contexts.last().unwrap();
        let inputs = context.control_source.bindings_from(cursor, |name| {
            self.env.get(name).filter(|_| context.carries_binding(name))
        });
        let path = self.residency_path();
        let path = match (self.iterators.last().and_then(Option::as_ref), cursor) {
            (Some((_, _, limit)), Some(cursor)) => {
                bounds::Condition::comparison(Comparison::Less, cursor, limit).assume(&path, true)
            }
            _ => path,
        };
        (path, inputs)
    }

    pub(super) async fn check_application_loop(&mut self, span: Span,cursor:Option<Value>) -> Result<()> {
        if self.loop_contexts.last().unwrap().initial {
            let state = self.loop_state(cursor);
            self.loop_contexts.last_mut().unwrap().continues.push(state);
            return Ok(());
        }
        let mut compatibility = None;
        let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let binding=self.env[&name].clone();
            let entry=self.loop_contexts.last().unwrap().entry.env.get(&name).cloned();
            if let Binding::OwnedValue(result) = &binding
                && let Some(Binding::OwnedValue(before)) = entry.as_ref()
            {
                if result.ownership != before.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的结果所有权，不能在回边重复消费",
                    );
                }
                if result.ownership == application::Ownership::Owned {
                    let (path, inputs) = compatibility.get_or_insert_with(|| self.loop_compatibility_inputs(cursor.as_ref()));
                    if !unsafe { self.world.call(before.loop_compatible(
                        result, self, span, path, inputs,
                    )).await }? {
                        return self.fail(span,"B266","循环回边尚未能合流结果载荷的分支与CPU／GPU存储位置；需要相同的资源生命周期");
                    }
                }
            }
            let data=match &binding{Binding::ApplicationData(data)=>Some(data),Binding::DataReference(data)=>Some(&data.data),_=>None};
            let before=match entry.as_ref(){Some(Binding::ApplicationData(data))=>Some(data),Some(Binding::DataReference(data))=>Some(&data.data),_=>None};
            if let(Some(data),Some(before))=(data,before) {
                if before.ownership != data.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的 dataset 所有权，不能在回边重复消费",
                    );
                }
                if before.ownership != application::Ownership::Moved {
                    let (path, inputs) = compatibility.get_or_insert_with(|| self.loop_compatibility_inputs(cursor.as_ref()));
                    if !unsafe { self.world.call(before.loop_compatible(data, self, span, path, inputs)).await }? {
                        return self.fail(span,"B266","循环继续前须归还入口及其拥有子资源相同的 CPU／GPU 存储位置；在循环内完成配对的显式传输");
                    }
                }
            }
        }
        let state = self.loop_state(cursor);
        self.loop_contexts.last_mut().unwrap().continues.push(state);
        Ok(())
    }
    pub(super) async fn release_application_loop(&mut self) -> Result<()> {
        let names = self.loop_contexts.last().unwrap().names.clone();
        unsafe { self.world.call(self.release_application_data(Some(&names),view::Borrow::default())).await }
    }
    pub(super) fn remember_loop_control(&mut self,value:&Value){
        let Some(context) = self.loop_contexts.last_mut() else { return; };
        context.controls.insert(value.regs[0], value.clone());
        let inputs = bounds::Condition::of(value).input_identities();
        for binding in self.env.values() {
            view::visit_data(binding, |data| {
                let id = data.length.identity();
                if let Some(Position::ApplicationInput(input, ..)) = &data.initialization
                    && inputs.contains(&id)
                    && let Some(context) = self.loop_contexts.iter_mut()
                        .find(|context| context.view_scope == Some(input.level))
                    && let InputSlot::Data { name, path, .. } = &context.view_inputs[input.index]
                {
                    context.control_source.inputs.insert(id, view_contracts::ControlInput::Length {
                        name: name.clone(), path: path.clone(),
                    });
                }
            });
        }
    }
    pub(super) fn enter_loop(&mut self, environment: &BTreeMap<String, Binding>) {
        // The saved entry still has its real length. Complete storage no longer
        // depends on that length's precision after loop writes are projected.
        let mut complete = Vec::new();
        for (name, binding) in environment {
            view::for_each_data_path(binding, |path, data, _| {
                if self.application_initialized(data) {
                    complete.push((name.clone(), path.to_vec()));
                }
            });
        }
        let mut resources=environment.clone();
        for(name,path)in complete {
            data_at_mut(resources.get_mut(&name).unwrap(),&path).unwrap().initialization=None;
            data_at_mut(self.env.get_mut(&name).unwrap(),&path).unwrap().initialization=None;
        }
        self.loop_contexts.push(LoopContext {
            first_register: self.registers.len(),
            first_use: self.view_model.uses.len(),
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
    pub(super) fn loop_state(&self,cursor:Option<Value>) -> LoopState {
        let context=self.loop_contexts.last().unwrap();
        LoopState {
            path: self.path.clone(),
            env: self
                .env
                .iter()
                .filter(|(name, _)| context.carries_binding(name))
                .map(|(n, b)| (n.clone(), b.clone()))
                .collect(),
            initialized: self.application_initialization_state(),
            control: self.control_uniform,
            cursor,
        }
    }
    fn restore_initial_edge(&mut self, state: LoopState) {
        self.env = state.env;
        if let(Some((_,current)),Some(cursor))=(self.loop_contexts.last_mut().unwrap().iterator.as_mut(),state.cursor){*current=cursor;}
        self.path = state.path;
        self.initialized = state.initialized;
        self.control_uniform = state.control;
    }
    pub(super) async fn expand_initial_loop<F>(
        &mut self,
        mut code: Vec<Inst>,
        next: bool,
        mut continuation: F,
    ) -> Result<(Vec<Inst>, Flow)>
    where
        F: std::ops::AsyncFnMut(&mut Self, Option<Value>) -> Result<()>,
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
        unsafe { self.world.call(expand_initial_code(
            self,
            &mut code,
            &mut breaks.into_iter(),
            &mut continues.into_iter(),
            &mut exits,
            &mut flow,
            &mut continuation,
        )).await }?;
        if let Some(state) = natural {
            code.extend(unsafe { self.world.call(compile_initial_continuation(
                self,
                state,
                &mut exits,
                &mut flow,
                &mut continuation,
            )).await }?);
        }
        self.loop_contexts.last_mut().unwrap().exits = exits;
        Ok((code, flow))
    }
    pub(super) fn emit_loop_break(&mut self) {
        self.emit(InstKind::Break);
        let state = self.loop_state(self.loop_contexts.last().unwrap().iterator.as_ref().map(|(_,cursor)|cursor.clone()));
        self.loop_contexts.last_mut().unwrap().exits.push(state);
        self.flow = Flow::BREAK;
    }

    pub(super) fn leave_loop(
        &mut self,
        mut natural:Option<LoopState>,
        test:&Value,
        condition:&[Inst],
        body:&[Inst],
        span:Span,
        nonempty:bool,
        range: Option<(&Value, &Value)>,
    )->Result<(bool,effects::LoopWrites)>{
        let mut context=self.loop_contexts.pop().unwrap();let natural_index=context.exits.len();
        let terminal = range.filter(|_| !context.initial && natural.is_some()
            && context.entry.env.values().any(|b| matches!(b, Binding::ApplicationData(_))))
            .map(|(begin, limit)| {
                let entered = bounds::Condition::comparison(Comparison::Less, begin, limit);
                let edges = context.continues.iter().map(|edge| {
                    let path = bounds::residency_scope(&edge.path, &edge.env);
                    let next = edge.cursor.as_ref().unwrap();
                    // A bounded successor can fail the next test only at the limit.
                    let path = if next.proof.domains(0).contains(&IndexDomain::Upper(limit.identity(), 0)) {
                        bounds::Condition::comparison(Comparison::Equal, next, limit).assume(&path, true)
                    } else {
                        bounds::Condition::comparison(Comparison::Less, next, limit).assume(&path, false)
                    };
                    path.possible().then_some(path)
                }).collect::<Vec<_>>();
                (entered, edges)
            });
        if let Some(scope) = context.view_scope {
            let backedge = context.continues.iter().map(|edge| edge.path.clone())
                .reduce(|left, right| bounds::Clause::either(&left, &right));
            self.view_model.history.insert(scope, view_contracts::LoopHistory {
                registers: context.first_register..self.registers.len(),
                uses: context.first_use..self.view_model.uses.len(),
                backedge,
            });
        }
        if !context.initial && let Some(exit)=natural.as_mut() {
            let empty=context.control_source.test.as_ref().unwrap().at_call(&context.control_source.bindings(&context.entry)).negate();
            let scalar_empty=std::cell::LazyCell::new(||if nonempty{bounds::Condition::constant(false)}else{match empty.truth_in(&context.entry.path){Some(truth)=>bounds::Condition::constant(truth),None=>empty.clone()}});
            let continue_scopes=context.continues.iter().map(|_|std::cell::OnceCell::new()).collect::<Vec<_>>();
            for(name,binding)in &mut exit.env {
                if let Binding::ApplicationData(data) = binding {
                    let written = matches!(&data.place, Some(Place::Local(_, slots, ..))
                        if slots.iter().any(|slot| context.writes.locals.contains(slot)));
                    if !written && terminal.is_none() {
                        continue;
                    }
                    let mut repeated: Option<returns::Returned> = None;
                    let mut location = residency::Residency::Absent;
                    for (index, edge) in context.continues.iter().enumerate() {
                        let binding = &edge.env[name];
                        if let Some((_, paths)) = &terminal
                            && let Some(path) = &paths[index]
                            && let Binding::ApplicationData(next) = binding
                        {
                            location.join(&next.residency.under(path));
                        }
                        if written {
                            let next = returns::Returned::Dataset(data_at(binding, &[]).unwrap().clone());
                            if let Some(common) = repeated.as_mut() {
                                common.join(next, true, None);
                            } else {
                                repeated = Some(next);
                            }
                        }
                    }
                    if let Some((entered, _)) = &terminal && !matches!(location, residency::Residency::Absent) {
                        data.residency = residency::Residency::conditional(
                            entered.clone(), location, std::mem::take(&mut data.residency),
                        );
                    }
                    if let Some(returns::Returned::Dataset(repeated)) = repeated {
                        if data.ownership != repeated.ownership {
                            data.ownership = application::Ownership::Moved;
                        }
                        if terminal.is_none() {
                            data.residency = residency::Residency::conditional(
                                empty.clone(), std::mem::take(&mut data.residency), repeated.residency,
                            );
                        }
                        data.elements = std::mem::take(&mut data.elements).selected(repeated.elements, &empty);
                        data.prefixes = prefix::join_fields(&data.prefixes, &repeated.prefixes, Some(&empty));
                    }
                    continue;
                }
                let Some((proof,uniform,place))=binding.proof_storage() else{continue;};
                let Some((entry,_,_))=context.entry.env.get_mut(name).and_then(Binding::proof_storage) else{continue;};
                let width=place.ty().components.len();
                for component in 0..width {
                    // A new header read is the final iteration's value. Other
                    // components take backedge facts only if the header leaves them unchanged.
                    if proof.known_identity(component).is_some_and(|id|entry.known_identity(component)!=Some(id)) || self.application_condition_writes(condition,&place,component){continue;}
                    let mut repeated:Option<(bounds::Proof,bounds::Clause,Reg)>=None;
                    for(edge,scope)in context.continues.iter_mut().zip(&continue_scopes) {
                        if let Some((next,next_uniform,_))=edge.env.get_mut(name).and_then(Binding::proof_storage) {
                            let next=next.project(&[component],width);*uniform &= *next_uniform;
                            let scope=scope.get_or_init(||bounds::proof_scope(&edge.path,&edge.env));
                            let next=scope(&Ty::scalar(place.ty().components[component].0),next);
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
                        let empty=&*scalar_empty;
                        let mut common=proof.project(&[component],width);
                        let aliases=[common.output_aliases(&[reg],&self.registers,&BTreeMap::new()),alias];
                        exit.path=bounds::Clause::both(&exit.path,&bounds::Clause::either(&empty.assume(&aliases[0],true),&empty.assume(&aliases[1],false)));
                        common.join_condition(&repeated,1,Some(empty));common={let mut component_proof=common.with_identity(ValueIdentity::Register(reg));component_proof.set_view_borrow(0,proof.view_borrow(component));component_proof};
                        proof.assign(&[component],width,common);
                    }
                }
            }
        }
        let has_natural = natural.is_some();
        context.exits.extend(natural);
        self.close_view_context(&mut context, has_natural.then_some(natural_index), nonempty)?;
        let guards = if context.exits.len() == 2 && has_natural
            && let Some((entered, paths)) = terminal
            && paths.iter().all(Option::is_none)
        {
            let mut test = test.clone();
            test.proof = bounds::Proof::condition(entered);
            let test = owned_presence::Presence::input(&test);
            vec![test.clone(), test.not()]
        } else if context.exits.len() > 1 {
            let test = owned_presence::Presence::input(test);
            let mut guards = loop_exit_paths(body, &context.controls, &test);
            if has_natural {
                guards.push(test.not());
            }
            guards
        } else {
            Vec::new()
        };
        let initialized = (context.exits.len() > 1).then(|| {
            ownership::merge_initialization(&mut context.exits, &guards, |state| &mut state.initialized)
        });
        let mut guards=guards.into_iter().rev().skip(1);let mut exits=context.exits.into_iter().rev();let Some(first)=exits.next()else{self.env.retain(|name,_|context.names.contains(name));self.path=bounds::Clause::impossible();return Ok((false,context.writes))};
        self.env=first.env;self.path=first.path;
        for exit in exits{let when=guards.next().unwrap();let mut previous=std::mem::replace(&mut self.env,exit.env);self.path=bounds::Clause::either(&self.path,&exit.path);self.join_environment_selected(&mut previous,&when,&when.condition());}
        self.env.retain(|name,_|context.names.contains(name));
        self.initialized = initialized.unwrap_or(first.initialized);
        self.view_uses.after(span);
        Ok((true,context.writes))
    }
}
use crate::ir::dataflow;
use owned_presence::Presence;
use std::ffi::c_void;

#[derive(Clone, Copy)]
#[repr(C)]
struct LoopPaths {
    live: *mut Presence,
    exited: *mut Presence,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct LoopPathHost {
    ir: dataflow::Host,
    context: *mut c_void,
    record_break: unsafe extern "C" fn(*mut c_void, *mut Presence),
    clear: unsafe extern "C" fn(*mut Presence),
    union_paths: unsafe extern "C" fn(*mut Presence, *mut Presence, *mut Presence),
    with_branch: unsafe extern "C" fn(Reg, dataflow::Span, dataflow::Span, LoopPaths, LoopPathHost),
    with_scope: unsafe extern "C" fn(dataflow::Span, LoopPaths, LoopPathHost),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::next_loop_edit"]
    fn next_loop_edit(code: dataflow::Span, first: usize, host: dataflow::Host) -> usize;

    #[link_name = "bootstrap.ir_dataflow::loop_exit_paths"]
    fn source_loop_exit_paths(code: dataflow::Span, paths: LoopPaths, host: LoopPathHost);

    #[link_name = "bootstrap.ir_dataflow::loop_path_branch"]
    fn source_loop_branch(yes: dataflow::Span, no: dataflow::Span, parent: LoopPaths, yes_paths: LoopPaths, no_paths: LoopPaths, host: LoopPathHost);

    #[link_name = "bootstrap.ir_dataflow::loop_path_scope"]
    fn source_loop_scope(code: dataflow::Span, parent: LoopPaths, child: LoopPaths, host: LoopPathHost);
}

struct ExitConditions<'a> {
    controls: &'a BTreeMap<Reg, Value>,
    test: &'a Presence,
    guards: &'a mut Vec<Presence>,
}

unsafe fn take_presence(slot: *mut Presence) -> Presence {
    std::mem::replace(unsafe { &mut *slot }, Presence::None)
}

unsafe extern "C" fn record_break(context: *mut c_void, live: *mut Presence) {
    let context = unsafe { &mut *context.cast::<ExitConditions<'_>>() };
    context.guards.push(context.test.clone().and(unsafe { take_presence(live) }));
}

unsafe extern "C" fn clear_loop_path(slot: *mut Presence) {
    unsafe { *slot = Presence::None; }
}

unsafe extern "C" fn union_loop_paths(output: *mut Presence, left: *mut Presence, right: *mut Presence) {
    let left = unsafe { take_presence(left) };
    let right = unsafe { take_presence(right) };
    unsafe { *output = left.or(right); }
}

unsafe extern "C" fn with_loop_branch(
    condition: Reg, yes: dataflow::Span, no: dataflow::Span, parent: LoopPaths, host: LoopPathHost,
) {
    let context = unsafe { &*host.context.cast::<ExitConditions<'_>>() };
    let selected = Presence::input(&context.controls[&condition]);
    let live = unsafe { take_presence(parent.live) };
    let mut yes_live = live.clone().and(selected.clone());
    let mut no_live = live.and(selected.not());
    let mut yes_exited = Presence::None;
    let mut no_exited = Presence::None;
    unsafe { source_loop_branch(yes, no, parent,
        LoopPaths { live: &mut yes_live, exited: &mut yes_exited },
        LoopPaths { live: &mut no_live, exited: &mut no_exited },
        host,
    ); }
}

unsafe extern "C" fn with_loop_scope(code: dataflow::Span, parent: LoopPaths, host: LoopPathHost) {
    let mut exited = Presence::None;
    unsafe { source_loop_scope(code, parent, LoopPaths { live: parent.live, exited: &mut exited }, host); }
}

unsafe extern "C" {
    #[link_name = "bootstrap.return_scope::loop_exit"]
    fn source_loop_exit(nodes: *const std::ffi::c_void, body: crate::syntax::wire::Slice) -> bool;
}

fn loop_exit_paths(code: &[Inst], controls: &BTreeMap<Reg, Value>, test: &Presence) -> Vec<Presence> {
    let mut guards = Vec::new();
    let mut context = ExitConditions { controls, test, guards: &mut guards };
    let mut live = Presence::All;
    let mut exited = Presence::None;
    let paths = LoopPaths { live: &mut live, exited: &mut exited };
    let host = LoopPathHost {
        ir: dataflow::HOST, context: std::ptr::from_mut(&mut context).cast(), record_break,
        clear: clear_loop_path, union_paths: union_loop_paths,
        with_branch: with_loop_branch, with_scope: with_loop_scope,
    };
    unsafe { source_loop_exit_paths(dataflow::Span::slice(code), paths, host); }
    guards
}

// Continue is the real next-iteration edge. Compile its continuation directly.
async fn compile_initial_continuation<'a, F>(
    builder: &mut Builder<'a>,
    state: LoopState,
    exits: &mut Vec<LoopState>,
    flow: &mut Flow,
    continuation: &mut F,
) -> Result<Vec<Inst>>
where
    F: std::ops::AsyncFnMut(&mut Builder<'a>, Option<Value>) -> Result<()>,
{
    let cursor = state.cursor.clone();
    builder.restore_initial_edge(state);
    builder.flow = Flow::NEXT;
    continuation(builder, cursor).await?;
    *flow = flow.union(builder.flow);
    let mut code = std::mem::take(&mut builder.code);
    if builder.flow.continues() {
        exits.push(builder.loop_state(builder.loop_contexts.last().unwrap().iterator.as_ref().map(|(_,cursor)|cursor.clone())));
        code.push(Inst::new(InstKind::Break));
    }
    Ok(code)
}
async fn expand_initial_code<'a, F>(
    builder: &mut Builder<'a>,
    code: &mut Vec<Inst>,
    breaks: &mut impl Iterator<Item = LoopState>,
    continues: &mut impl Iterator<Item = LoopState>,
    exits: &mut Vec<LoopState>,
    flow: &mut Flow,
    continuation: &mut F,
) -> Result<()>
where
    F: std::ops::AsyncFnMut(&mut Builder<'a>, Option<Value>) -> Result<()>,
{
    let mut first = 0;
    loop {
        let index = unsafe { next_loop_edit(dataflow::Span::slice(code), first, dataflow::HOST) };
        let Some(instruction) = code.get_mut(index) else {
            return Ok(());
        };
        first = index + 1;
        match &mut instruction.kind {
            InstKind::Break => exits.push(breaks.next().unwrap()),
            InstKind::Continue => {
                let next = unsafe { builder.world.call(compile_initial_continuation(
                    builder,
                    continues.next().unwrap(),
                    exits,
                    flow,
                    continuation,
                )).await }?;
                first = index + next.len();
                drop(code.splice(index..index + 1, next));
            }
            InstKind::If { yes, no, .. } => {
                unsafe { builder.world.call(expand_initial_code(
                    builder, yes, breaks, continues, exits, flow, continuation,
                )).await }?;
                unsafe { builder.world.call(expand_initial_code(
                    builder, no, breaks, continues, exits, flow, continuation,
                )).await }?;
            }
            InstKind::Scope(body) => {
                unsafe { builder.world.call(expand_initial_code(
                    builder, body, breaks, continues, exits, flow, continuation,
                )).await }?;
            }
            _ => {}
        }
    }
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
            if states.iter().all(|state|scope.application_word_initialized(Some(iteration.index),*word,state)){
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


#[repr(C)]
#[derive(Clone,Copy)]
struct WhileCursor {phase:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct WhileFacts {initial:u32,present:u32,truth:u32,possible:u32,continuing:u32,outer:u32,condition:u32,flow:u32,next:u32}
#[repr(u32)]
enum WhileNeed {Enter,Writes,Condition,Empty,Inspect,Refine,Bounds,NextBound,Bound,Retain,Impossible,SaveBody,Natural,RestoreBody,EnterBody,Body,CheckBackedge,Split,Expand,Leave,Emit}
#[repr(C)]
struct WhileStep {cursor:WhileCursor,need:WhileNeed,uniform:u32,flow:u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::while_step"] fn while_step(cursor:WhileCursor,facts:WhileFacts)->WhileStep;
}
impl Builder<'_> {
 pub(super) async fn while_loop(&mut self,condition:&Expr,body:&Block,initial:bool,span:Span)->Result<()> {
  let mut cursor=WhileCursor {phase:0};let mut facts=WhileFacts {initial:u32::from(initial),present:0,truth:0,possible:0,continuing:0,outer:0,condition:0,flow:0,next:0};
  let mut environment=None;let mut c=None;let mut partition=None;let mut condition_prefix=0;let mut condition_env=None;let mut condition_path=None;
  let mut bounds=None;let mut bound=None;let mut body_env=None;let mut body_path=None;let mut natural=None;let mut control=false;
  let mut body_flow=Flow::NEXT;let mut code=None;let mut condition_code=None;let mut outer=None;
  loop {
   let step=unsafe {while_step(cursor,facts)};cursor=step.cursor;
   match step.need {
    WhileNeed::Enter=>{
     self.flow=Flow::NEXT;let saved=self.env.clone();self.enter_loop(&saved);self.configure_initial_loop(initial,None,None);self.prepare_view_loop();environment=Some(saved);
    },
    WhileNeed::Writes=>{let writes=unsafe {self.world.call(self.loop_writes(Some(condition),body,None,initial)).await}?;self.loop_contexts.last_mut().unwrap().writes=writes;},
    WhileNeed::Condition=>{
     self.loop_contexts.last_mut().unwrap().outer=std::mem::take(&mut self.code);
     c=unsafe {self.world.call(self.temporary_scope(condition.span(),async |builder|Ok((unsafe {builder.world.call(builder.expr(condition)).await}?,view::Borrow::default())))).await}?;
     facts.present=u32::from(c.is_some());
    },
    WhileNeed::Empty=>{
     let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);self.finish_view_condition()?;
     let code=std::mem::replace(&mut self.code,outer);self.code.extend(code);return Ok(());
    },
    WhileNeed::Inspect=>{
     let value=c.as_ref().unwrap();self.require_bool(value,condition.span())?;
     self.loop_contexts.last_mut().unwrap().control_source=view_contracts::LoopControl::capture(value,&self.env,None);
     partition=Some(Arc::new(bounds::Condition::of(value)));condition_prefix=self.code.len();condition_env=Some(self.env.clone());condition_path=Some(self.path.clone());
     facts.truth=value.constant.as_ref().map_or(0,|bits|if bits[0]==0{1}else{2});facts.condition=u32::from(value.uniform);
    },
    WhileNeed::Refine=>self.refine_bounds(partition.as_ref().unwrap(),true),
    WhileNeed::Bounds=>bounds=Some(std::mem::take(&mut self.loop_contexts.last_mut().unwrap().writes.body_bounds).into_iter()),
    WhileNeed::NextBound=>{bound=bounds.as_mut().unwrap().next();facts.present=u32::from(bound.is_some());},
    WhileNeed::Bound=>{
     let(name,(low,high))=bound.take().unwrap();if let Some(Binding::Owned(owner))=self.env.get_mut(&name){owner.proof.retain_loop_bounds(&bounds::Proof::unsigned(low,high),0,true,true);}
    },
    WhileNeed::Retain=>{bounds=None;facts.possible=u32::from(unsafe {self.world.call(self.retain_loop_bounds(Some(condition),body,environment.as_ref().unwrap(),condition_env.as_mut().unwrap())).await}?);},
    WhileNeed::Impossible=>{
     self.env=condition_env.take().unwrap();self.path=condition_path.take().unwrap();let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
     self.finish_view_condition()?;self.refine_bounds(partition.as_ref().unwrap(),false);let code=std::mem::replace(&mut self.code,outer);self.code.extend(code);return Ok(());
    },
    WhileNeed::SaveBody=>{body_env=Some(std::mem::replace(&mut self.env,condition_env.take().unwrap()));body_path=Some(std::mem::replace(&mut self.path,condition_path.take().unwrap()));environment=None;},
    WhileNeed::Natural=>{self.refine_bounds(partition.as_ref().unwrap(),false);natural=Some(self.loop_state(None));},
    WhileNeed::RestoreBody=>{self.env=body_env.take().unwrap();self.path=body_path.take().unwrap();facts.outer=u32::from(self.control_uniform);},
    WhileNeed::EnterBody=>{control=self.control_uniform;self.control_uniform=step.uniform!=0;self.loop_depth+=1;self.iterators.push(None);},
    WhileNeed::Body=>{unsafe {self.world.call(self.block(body)).await}?;body_flow=self.flow;facts.continuing=u32::from(body_flow.continues());facts.flow=u32::from(body_flow.0);},
    WhileNeed::CheckBackedge=>{unsafe {self.world.call(self.check_application_loop(condition.span(),None)).await}?;},
    WhileNeed::Split=>{self.iterators.pop();self.loop_depth-=1;code=Some(self.code.split_off(condition_prefix));condition_code=Some(std::mem::take(&mut self.code));},
    WhileNeed::Expand=>{
     let(expanded,flow)=unsafe {self.world.call(self.expand_initial_loop(code.take().unwrap(),body_flow.continues(),async |builder,_|{
      let initial=unsafe {builder.world.call(builder.next_initial_loop(body)).await}?;
      unsafe {builder.world.call(builder.while_loop(condition,body,initial,span)).await}
     })).await}?;code=Some(expanded);body_flow=body_flow.union(flow);facts.flow=u32::from(body_flow.0);
    },
    WhileNeed::Leave=>{
     outer=Some(std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer));
     let(next,_)=self.leave_loop(natural.take(),c.as_ref().unwrap(),condition_code.as_ref().unwrap(),code.as_ref().unwrap(),span,false,None)?;facts.next=u32::from(next);
    },
    WhileNeed::Emit=>{
     self.flow=Flow(step.flow as u8);self.control_uniform=control;self.code=outer.take().unwrap();
     self.emit(InstKind::Loop {condition:condition_code.take().unwrap(),test:c.as_ref().unwrap().regs[0],body:code.take().unwrap()});return Ok(());
    },
   }
  }
 }
}

#[repr(C)]
#[derive(Clone,Copy)]
struct ForCursor {phase:u32,initial:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct ForFacts {initial:u32,minimum:u32,maximum:u32,same:u32,seeks:u32,domain:u32,present:u32,outer:u32,begin_uniform:u32,limit_uniform:u32,flow:u32,next:u32,nonempty:u32}
#[repr(u32)]
enum ForNeed {EmptyBegin,EmptyNext,EmptyInitialize,EmptyDone,Seeks,ClearInitialization,Enter,Cursor,Test,BodyCursor,RefineStart,Application,ApplicationInput,ApplicationBinding,DatasetBinding,TileBinding,ViewBinding,NextComponent,Component,FinishOwned,FinishView,IndexBinding,Install,Writes,Capture,Natural,Retain,EnterBody,Body,Advance,CheckBackedge,EndBody,IterationElements,ExitInitialization,TestEntry,DropNatural,Expand,Leave,Emit}
#[repr(C)]
struct ForStep {cursor:ForCursor,need:ForNeed,uniform:u32,flow:u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::for_step"] fn for_step(cursor:ForCursor,facts:ForFacts)->ForStep;
}
impl Builder<'_> {
 pub(super) async fn for_loop(&mut self,cursor_specification:ForLoop,body:&Block,span:Span,initial:bool,nonempty:bool,original_begin:&Value)->Result<effects::LoopWrites> {
  self.flow=Flow::NEXT;
  let ForLoop {name,begin,limit,data,borrowed,slot,mut initialization}=cursor_specification.clone();
  let(mut application,mut dataset,mut tile,mut view,domain)=match data {
   Some(CursorSource::ApplicationData(value))=>(Some(value),None,None,None,1),
   Some(CursorSource::Data(value))=>(None,Some(value),None,None,2),
   Some(CursorSource::Tile(value))=>(None,None,Some(value),None,3),
   Some(CursorSource::View(value))=>(None,None,None,Some(value),4),
   None=>(None,None,None,None,0),
  };
  let mut state=ForCursor {phase:0,initial:0};
  let mut facts=ForFacts {initial:u32::from(initial),minimum:begin.minimum(),maximum:limit.maximum(),same:u32::from(begin.identity()==limit.identity()),seeks:0,domain,present:0,outer:0,begin_uniform:u32::from(begin.uniform),limit_uniform:u32::from(limit.uniform),flow:0,next:0,nonempty:u32::from(nonempty)};
  let mut empty=None;let mut empty_item=None;let mut environment=None;let mut index=0;let mut cursor=None;let mut body_cursor=None;let mut test=None;let mut condition=None;
  let mut binding=None;let mut place=None;let mut input=bounds::Proof::default();let mut component=0;let mut natural=None;let mut iteration=None;
  let mut limit=Some(limit);let mut control=false;let mut body_flow=Flow::NEXT;let mut body_reachable=false;let mut successor=None;let mut code=None;let mut outer=None;let mut writes=None;
  loop {
   let step=unsafe {for_step(state,facts)};state=step.cursor;let initial=state.initial!=0;
   match step.need {
    ForNeed::EmptyBegin=>empty=Some(std::mem::take(&mut initialization).into_iter()),
    ForNeed::EmptyNext=>{empty_item=empty.as_mut().unwrap().next();facts.present=u32::from(empty_item.is_some());},
    ForNeed::EmptyInitialize=>{let(data,words)=empty_item.take().unwrap();self.initialize_application_words(&data,&words);},
    ForNeed::EmptyDone=>return Ok(effects::LoopWrites::default()),
    ForNeed::Seeks=>facts.seeks=u32::from(bounds::seeks(body)),
    ForNeed::ClearInitialization=>initialization.clear(),
    ForNeed::Enter=>{
     let saved=self.env.clone();self.enter_loop(&saved);self.configure_initial_loop(initial,Some(name.clone()),Some(begin.clone()));self.prepare_view_loop();
     self.loop_contexts.last_mut().unwrap().outer=std::mem::take(&mut self.code);environment=Some(saved);
    },
    ForNeed::Cursor=>{
     index=self.reg(Scalar::U32);self.emit(InstKind::LoadLocal(index,slot));
     cursor=Some(Value {proof:if initial {bounds::Proof::capture(&begin)}else{bounds::Proof::default()},literal:false,ty:Ty::scalar(Scalar::U32),regs:vec![index],uniform:step.uniform!=0,constant:if initial {begin.constant.clone()}else{None}});
    },
    ForNeed::Test=>{test=Some(self.binary("<",cursor.as_ref().unwrap().clone(),limit.as_ref().unwrap().clone(),span,None)?);condition=Some(std::mem::take(&mut self.code));},
    ForNeed::BodyCursor=>{body_cursor=Some(Value {proof:if initial {bounds::Proof::capture(&begin)}else{bounds::for_cursor(&begin,limit.as_ref().unwrap(),facts.seeks!=0)},..cursor.as_ref().unwrap().clone()});},
    ForNeed::RefineStart=>{self.refine_bounds(&Arc::new(bounds::Condition::comparison(Comparison::LessEqual,original_begin,cursor.as_ref().unwrap())),true);},
    ForNeed::Application=>{
     let data=application.as_mut().unwrap();data.data.root.proof.set_view_borrow(0,data.root.proof.view_borrow(0).joined(self.data_borrow(&data.sources)));
    },
    ForNeed::ApplicationInput=>{input=unsafe {self.world.call(self.application_cursor_input(application.as_ref().unwrap(),body_cursor.as_ref().unwrap())).await}?;},
    ForNeed::ApplicationBinding=>{
     let data=application.take().unwrap();let value=cursor.as_ref().unwrap();binding=Some(Binding::Owned(OwnedBinding {place:data.element(value,(value.minimum(),value.maximum().min(data.length.maximum().saturating_sub(1)))),proof:std::mem::take(&mut input),uniform:false,initialization:None}));
    },
    ForNeed::DatasetBinding=>{binding=Some(Binding::Owned(OwnedBinding {place:self.dataset_element(&dataset.take().unwrap(),index,false),proof:bounds::Proof::default(),uniform:false,initialization:None}));},
    ForNeed::TileBinding=>{let value=tile.take().unwrap();place=Some(Place::TileElement {ty:value.ty,tile:value.tile,index,words:value.words});},
    ForNeed::ViewBinding=>place=Some(Self::view_element(view.as_ref().unwrap(),index)),
    ForNeed::NextComponent=>facts.present=u32::from(component<place.as_ref().unwrap().ty().components.len()),
    ForNeed::Component=>{
     let ty=place.as_ref().unwrap().ty();let width=ty.components.len();let scalar=ty.components[component].0;
     input.assign(&[component],width,bounds::Proof::default().with_identity(ValueIdentity::Register(self.reg(scalar))));component+=1;
    },
    ForNeed::FinishOwned=>binding=Some(Binding::Owned(OwnedBinding {place:place.take().unwrap(),proof:std::mem::take(&mut input),uniform:false,initialization:None})),
    ForNeed::FinishView=>binding=Some(Binding::ViewElement {element:OwnedBinding {place:place.take().unwrap(),proof:std::mem::take(&mut input),uniform:false,initialization:None},view:view.take().unwrap()}),
    ForNeed::IndexBinding=>binding=Some(Binding::Value(body_cursor.as_ref().unwrap().clone())),
    ForNeed::Install=>{self.env.insert(name.clone(),binding.take().unwrap());},
    ForNeed::Writes=>{let mut writes=unsafe {self.world.call(self.loop_writes(None,body,borrowed,initial)).await}?;writes.locals.insert(slot);self.loop_contexts.last_mut().unwrap().writes=writes;},
    ForNeed::Capture=>{
     self.loop_contexts.last_mut().unwrap().control_source=view_contracts::LoopControl::capture(test.as_ref().unwrap(),&self.env,Some(body_cursor.as_ref().unwrap().clone()));
     self.loop_contexts.last_mut().unwrap().iterator.as_mut().unwrap().1=body_cursor.take().unwrap();
     iteration=Some(match self.env.get(&name) {Some(Binding::Value(value))=>ownership::ApplicationIndex::of(value),_=>ownership::ApplicationIndex::of(cursor.as_ref().unwrap())});
    },
    ForNeed::Natural=>{
     let condition_env=self.env.clone();let condition_path=self.path.clone();let partition=Arc::new(bounds::Condition::of(test.as_ref().unwrap()));self.refine_bounds(&partition,false);natural=Some(self.loop_state(self.loop_contexts.last().unwrap().iterator.as_ref().map(|(_,cursor)|cursor.clone())));
     self.env=condition_env;self.path=condition_path;self.refine_bounds(&partition,true);facts.outer=u32::from(self.control_uniform);
    },
    ForNeed::Retain=>{unsafe {self.world.call(self.retain_loop_bounds(None,body,environment.as_ref().unwrap(),&mut natural.as_mut().unwrap().env)).await}?;facts.outer=u32::from(self.control_uniform);},
    ForNeed::EnterBody=>{environment=None;control=self.control_uniform;self.control_uniform=step.uniform!=0;self.loop_depth+=1;self.iterators.push(Some((name.clone(),slot,limit.take().unwrap())));},
    ForNeed::Body=>{unsafe {self.world.call(self.block(body)).await}?;body_flow=self.flow;body_reachable=body_flow.continues();facts.flow=u32::from(body_flow.0);},
    ForNeed::Advance=>successor=Some(self.advance_iterator(slot)?),
    ForNeed::CheckBackedge=>{unsafe {self.world.call(self.check_application_loop(span,Some(successor.take().unwrap()))).await}?;},
    ForNeed::EndBody=>{let(_,_,value)=self.iterators.pop().unwrap().unwrap();limit=Some(value);self.loop_depth-=1;},
    ForNeed::IterationElements=>{
     let source=match &cursor_specification.data {Some(CursorSource::ApplicationData(data))=>Some(data),_=>None};
     unsafe {self.world.call(self.application_iteration_elements(natural.as_mut().unwrap(),source,cursor.as_ref().unwrap(),limit.as_ref().unwrap())).await}?;
    },
    ForNeed::ExitInitialization=>{
     code=Some(std::mem::take(&mut self.code));let natural=natural.as_mut().unwrap();let iteration=iteration.unwrap();let limit=limit.as_ref().unwrap();
     self.initialize_iteration_exit(natural,&initialization,iteration,initial);self.initialize_array_iteration_exit(natural,&begin,limit,iteration,initial,body);self.defer_iteration_exit(natural,&begin,limit,&initialization,iteration,initial,body);
    },
    ForNeed::TestEntry=>{test.as_mut().unwrap().proof=bounds::Proof::condition(bounds::Condition::comparison(Comparison::Less,&begin,limit.as_ref().unwrap()));},
    ForNeed::DropNatural=>natural=None,
    ForNeed::Expand=>{
     let specification=cursor_specification.clone();let iteration=iteration.unwrap();
     let(expanded,flow)=unsafe {self.world.call(self.expand_initial_loop(code.take().unwrap(),body_reachable,async |builder,next|{
      let mut cursor=specification.clone();cursor.initialization=builder.application_iteration_prefix(&initialization,iteration);cursor.begin=next.unwrap();
      let initial=unsafe {builder.world.call(builder.next_initial_loop(body)).await}?;let nonempty=cursor.begin.maximum()<cursor.limit.minimum();
      unsafe {builder.world.call(builder.for_loop(cursor,body,span,initial,nonempty,original_begin)).await}.map(|_|())
     })).await}?;code=Some(expanded);body_flow=body_flow.union(flow);facts.flow=u32::from(body_flow.0);
    },
    ForNeed::Leave=>{
     outer=Some(std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer));
     let(next,result)=self.leave_loop(natural.take(),test.as_ref().unwrap(),condition.as_ref().unwrap(),code.as_ref().unwrap(),span,nonempty,Some((&begin,limit.as_ref().unwrap())))?;facts.next=u32::from(next);writes=Some(result);
    },
    ForNeed::Emit=>{
     self.flow=Flow(step.flow as u8);self.control_uniform=control;self.code=outer.take().unwrap();self.emit(InstKind::Loop {condition:condition.take().unwrap(),test:test.as_ref().unwrap().regs[0],body:code.take().unwrap()});return Ok(writes.take().unwrap());
    },
   }
  }
 }
}

#[repr(C)]
#[derive(Clone,Copy)]
struct ForInputCursor {phase:u32,nonempty:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct ForInputFacts {collision:u32,range:u32,present:u32,name:u32,kind:u32,access:u32,view:u32,begin_min:u32,begin_max:u32,limit_min:u32,limit_max:u32,continuing:u32}
#[repr(u32)]
enum ForInputNeed {Collision,RangeBegin,RangeEnd,BeginType,EndType,ApplicationReference,SourceKind,NameBinding,DatasetReference,Zero,Classify,Application,Dataset,Tile,ViewExpression,InvalidAccess,InvalidSource,View,Bounds,Borrowed,Initial,Slot,Initialization,BindApplication,Loop,CompleteWrites,Complete}
#[repr(C)]
struct ForInputStep {cursor:ForInputCursor,need:ForInputNeed}
#[repr(C)]
struct ForWrittenHost<'a,'w> {context:*mut ForWritten<'a,'w>,storage:unsafe extern "C" fn(*mut ForWritten<'a,'w>,usize)->u32,locals:unsafe extern "C" fn(*mut ForWritten<'a,'w>)->usize,local:unsafe extern "C" fn(*mut ForWritten<'a,'w>,usize)->u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::for_input_step"] fn for_input_step(cursor:ForInputCursor,facts:ForInputFacts)->ForInputStep;
 #[link_name="bootstrap.function_body::for_written"] fn for_written(unknown:u32,count:usize,host:ForWrittenHost<'_,'_>)->u32;
}
struct ForWritten<'a,'w> {builder:&'a Builder<'w>,writes:&'a effects::LoopWrites,data:&'a application::Data,slots:&'a [usize]}
unsafe extern "C" fn for_written_storage(context:*mut ForWritten<'_,'_>,index:usize)->u32 {
 let v=unsafe {&*context};u32::from(v.writes.storage[index].includes_root(v.builder,v.data.root.regs[0]))
}
unsafe extern "C" fn for_written_locals(context:*mut ForWritten<'_,'_>)->usize {
 let v=unsafe {&mut *context};v.slots=match v.data.place.as_ref().map(Place::storage) {Some(Place::Local(_,slots,_))=>slots.as_slice(),_=>&[]};v.slots.len()
}
unsafe extern "C" fn for_written_local(context:*mut ForWritten<'_,'_>,index:usize)->u32 {
 let v=unsafe {&*context};u32::from(v.writes.locals.contains(&v.slots[index]))
}
impl Builder<'_> {
 #[inline(never)]
 pub(super) async fn for_statement(&mut self,name:&str,start:&Expr,end:&Option<Expr>,body:&Block,span:Span)->Result<()> {
  let mut cursor=ForInputCursor {phase:0,nonempty:0};let mut facts=ForInputFacts {collision:u32::from(self.env.contains_key(name)),range:u32::from(end.is_some()),present:0,name:0,kind:0,access:0,view:0,begin_min:0,begin_max:0,limit_min:0,limit_max:0,continuing:0};
  let mut begin=None;let mut limit=None;let mut source=None;let mut source_name=None;let mut application=None;let mut dataset=None;let mut tile=None;let mut value=None;let mut data=None;
  let mut borrowed=None;let mut initial=false;let mut slot=0;let mut initialization=None;let mut writes=None;
  loop {
   let step=unsafe {for_input_step(cursor,facts)};cursor=step.cursor;
   match step.need {
    ForInputNeed::Collision=>return self.fail(span,"B125","循环变量不能覆盖已有绑定"),
    ForInputNeed::RangeBegin=>{
     begin=unsafe {self.world.call(self.temporary_scope(start.span(),async |builder|Ok((unsafe {builder.world.call(builder.expr(start)).await}?,view::Borrow::default())))).await}?;facts.present=u32::from(begin.is_some());
    },
    ForInputNeed::RangeEnd=>{
     let end=end.as_ref().unwrap();limit=unsafe {self.world.call(self.temporary_scope(end.span(),async |builder|Ok((unsafe {builder.world.call(builder.expr(end)).await}?,view::Borrow::default())))).await}?;facts.present=u32::from(limit.is_some());
    },
    ForInputNeed::BeginType=>self.require_same(&Ty::scalar(Scalar::U32),&begin.as_ref().unwrap().ty,start.span())?,
    ForInputNeed::EndType=>self.require_same(&begin.as_ref().unwrap().ty,&limit.as_ref().unwrap().ty,start.span())?,
    ForInputNeed::ApplicationReference=>{source=unsafe {self.world.call(self.application_reference(start)).await}?.map(Binding::DataReference);facts.present=u32::from(source.is_some());},
    ForInputNeed::SourceKind=>{source_name=match start.kind(){ExprKind::Name(name)=>Some(name),_=>None};facts.name=u32::from(source_name.is_some());},
    ForInputNeed::NameBinding=>source=self.env.get(source_name.take().unwrap()).cloned(),
    ForInputNeed::DatasetReference=>source=self.dataset_reference(start)?.map(Binding::Data),
    ForInputNeed::Zero=>begin=Some(self.constant(Scalar::U32,0)),
    ForInputNeed::Classify=>{
     match source.take() {
      Some(Binding::DataReference(v))=>{application=Some(v);facts.kind=1;},
      Some(Binding::Data(v))=>{facts.access=v.access as u32;dataset=Some(v);facts.kind=2;},
      Some(Binding::Tile(v))=>{tile=Some(v);facts.kind=3;},
      _=>facts.kind=0,
     }
    },
    ForInputNeed::Application=>{let data=application.as_ref().unwrap();self.application_cpu(data,start.span())?;limit=Some(data.length.clone());},
    ForInputNeed::Dataset=>{let v=dataset.take().unwrap();limit=Some(self.data_length(&v));data=Some(CursorSource::Data(v));},
    ForInputNeed::Tile=>{let v=tile.take().unwrap();limit=Some(self.tile_length(&v));data=Some(CursorSource::Tile(v));},
    ForInputNeed::ViewExpression=>{value=unsafe {self.world.call(self.expr(start)).await}?;facts.present=u32::from(value.is_some());facts.view=u32::from(value.as_ref().is_some_and(|v|v.ty.view.is_some()));},
    ForInputNeed::InvalidAccess=>return self.fail(start.span(),"B128","只读顺序遍历需要 read／inout 或 atomic 数据集；atomic 游标仍须显式原子访问"),
    ForInputNeed::InvalidSource=>return self.fail(start.span(),"B127","顺序遍历需要 dataset、tile 或不可变 view"),
    ForInputNeed::View=>{let v=value.take().unwrap();limit=Some(Self::view_length(&v));data=Some(CursorSource::View(v));},
    ForInputNeed::Bounds=>{let a=begin.as_ref().unwrap();let b=limit.as_ref().unwrap();facts.begin_min=a.minimum();facts.begin_max=a.maximum();facts.limit_min=b.minimum();facts.limit_max=b.maximum();},
    ForInputNeed::Borrowed=>borrowed=Some(application.as_ref().unwrap().root.identity()),
    ForInputNeed::Initial=>initial=unsafe {self.world.call(self.application_initial_loop(body)).await}?,
    ForInputNeed::Slot=>{slot=self.local_slots(&Ty::scalar(Scalar::U32)).0[0];self.emit(InstKind::StoreLocal(slot,begin.as_ref().unwrap().regs[0]));},
    ForInputNeed::Initialization=>initialization=Some(self.application_iteration_outputs(begin.as_ref().unwrap(),limit.as_ref().unwrap(),application.as_ref().map(|v|&v.data))),
    ForInputNeed::BindApplication=>data=Some(CursorSource::ApplicationData(application.take().unwrap())),
    ForInputNeed::Loop=>{
     writes=Some(unsafe {self.world.call(self.for_loop(ForLoop {name:name.to_string(),begin:begin.as_ref().unwrap().clone(),limit:limit.take().unwrap(),data:data.take(),borrowed,slot,initialization:initialization.take().unwrap()},body,span,initial,cursor.nonempty!=0,begin.as_ref().unwrap())).await}?);facts.continuing=u32::from(self.flow.continues());
    },
    ForInputNeed::CompleteWrites=>{
     let writes=writes.as_ref().unwrap();unsafe {self.world.call(self.complete_application_writes(|builder,data|{
      let mut context=ForWritten {builder,writes,data,slots:&[]};
      (unsafe {for_written(u32::from(writes.unknown_storage),writes.storage.len(),ForWrittenHost {context:&mut context,storage:for_written_storage,locals:for_written_locals,local:for_written_local})})!=0
     },span)).await}?;
    },
    ForInputNeed::Complete=>return Ok(()),
   }
  }
 }
}

#[repr(C)]#[derive(Clone,Copy)]pub(super) struct LocalCursor {pub(super) phase:u32}
#[repr(C)]#[derive(Clone,Copy)]struct LetFacts {collision:u32,annotation:u32,kind:u32,publish:u32,tile:u32,continuing:u32,present:u32}
#[repr(u32)]enum LetNeed {Collision,Inspect,Publish,Expected,Collective,Cooperate,Tile,Evaluate,Missing,CheckType,Bind,Complete}
#[repr(C)]struct LetStep {cursor:LocalCursor,need:LetNeed,atomic:u32}
#[repr(C)]#[derive(Clone,Copy)]struct DeclareFacts {collision:u32,dataset:u32,owns:u32}
#[repr(u32)]enum DeclareNeed {Collision,Dataset,Type,Owned,Plain}
#[repr(C)]struct DeclareStep {cursor:LocalCursor,need:DeclareNeed}
#[repr(C)]#[derive(Clone,Copy)]struct ValueCursor {phase:u32,index:usize}
#[repr(C)]#[derive(Clone,Copy)]struct ValueFacts {collision:u32,mutable:u32,arrays:u32,uniform:u32,control:u32,slots:usize,regs:usize}
#[repr(u32)]enum ValueNeed {View,Collision,Mutability,Arrays,Array,ArrayFinish,Immutable,Slots,Store,Finish}
#[repr(C)]struct ValueStep {cursor:ValueCursor,need:ValueNeed,uniform:u32,index:usize}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::local_publish"]fn local_publish(bytes:*const u8,length:usize)->u32;
 #[link_name="bootstrap.function_body::let_step"]fn let_step(cursor:LocalCursor,facts:LetFacts)->LetStep;
 #[link_name="bootstrap.function_body::declare_step"]fn declare_step(cursor:LocalCursor,facts:DeclareFacts)->DeclareStep;
 #[link_name="bootstrap.function_body::value_step"]fn value_step(cursor:ValueCursor,facts:ValueFacts)->ValueStep;
}
impl Builder<'_> {
 #[inline(never)]
 pub(super) async fn let_statement(&mut self,annotation:&Option<TypeName>,name:&str,e:&Expr,span:Span)->Result<()> {
  let mut cursor=LocalCursor {phase:0};let mut facts=LetFacts {collision:u32::from(self.env.contains_key(name)),annotation:u32::from(annotation.is_some()),kind:0,publish:0,tile:0,continuing:0,present:0};
  let mut call=None;let mut cooperative=None;let mut expected=None;let mut op=None;let mut result=None;
  loop {
   facts.continuing=u32::from(self.flow.continues());facts.present=u32::from(result.is_some());let step=unsafe {let_step(cursor,facts)};cursor=step.cursor;
   match step.need {
    LetNeed::Collision=>return self.fail(span,"B125",format!("重复局部变量或覆盖参数 {name}")),
    LetNeed::Inspect=>match e.kind(){
     ExprKind::Call(name,args)=>{facts.kind=1;facts.publish=unsafe {local_publish(name.as_ptr(),name.len())};call=Some((name,args));},
     ExprKind::Cooperate(name,explicit,args)=>{facts.kind=2;cooperative=Some((name,explicit,args));},
     _=>facts.kind=0,
    },
    LetNeed::Publish=>{let(_,args)=call.as_ref().unwrap();return unsafe {self.world.call(self.tile_publish(annotation.is_some(),name,args,span,step.atomic!=0)).await};},
    LetNeed::Expected=>expected=Some(unsafe {self.world.call(self.expected_storage(annotation.as_ref().unwrap(),span)).await}?),
    LetNeed::Collective=>{op=collective::from_source(call.as_ref().unwrap().0).tile();facts.tile=u32::from(op.is_some());},
    LetNeed::Cooperate=>{let(name,explicit,args)=cooperative.as_ref().unwrap();result=unsafe {self.world.call(self.cooperate(name,explicit.as_ref(),args,e.span())).await}?.map(returns::Returned::value);},
    LetNeed::Tile=>{result=Some(returns::Returned::value(self.tile_collective(op.take().unwrap(),&call.as_ref().unwrap().1,span)?));},
    LetNeed::Evaluate=>result=unsafe {self.world.call(self.evaluate_returned(e,expected.as_ref())).await}?,
    LetNeed::Missing=>return self.fail(span,"B228","局部绑定需要值或拥有资源"),
    LetNeed::CheckType=>{unsafe {self.world.call(self.check_returned_type(result.as_mut().unwrap(),expected.as_ref().unwrap(),span)).await}?;},
    LetNeed::Bind=>return self.bind_returned(name,result.take().unwrap(),span),
    LetNeed::Complete=>return Ok(()),
   }
  }
 }
 #[inline(never)]
 pub(super) async fn declare_statement(&mut self,annotation:&TypeName,name:&str,span:Span)->Result<()> {
  let mut cursor=LocalCursor {phase:0};let mut facts=DeclareFacts {collision:u32::from(self.env.contains_key(name)),dataset:u32::from(annotation.dataset),owns:0};let mut ty=None;
  loop {let step=unsafe {declare_step(cursor,facts)};cursor=step.cursor;match step.need {
   DeclareNeed::Collision=>return self.fail(span,"B125",format!("重复局部变量或覆盖参数 {name}")),
   DeclareNeed::Dataset=>return unsafe {self.world.call(self.application_declare(annotation,name,span)).await},
   DeclareNeed::Type=>{let value=unsafe {self.world.call(self.world.ty(annotation,self.scope.source(),span,self.scope.module(),self.scope.imports())).await}?;facts.owns=u32::from(value.owns_resources());ty=Some(value);},
   DeclareNeed::Owned=>return unsafe {self.world.call(self.owned_value_declare(name,ty.take().unwrap(),span)).await},
   DeclareNeed::Plain=>{let ty=ty.take().unwrap();let(slots,storage)=self.local_slots(&ty);self.env.insert(name.to_string(),Binding::Owned(OwnedBinding {place:Place::Local(ty,slots,storage),proof:bounds::Proof::default(),uniform:false,initialization:None}));return Ok(());},
  }}
 }
 pub(super) fn bind_local_value(&mut self,name:&str,value:Value,span:Span)->Result<()> {
  let mut cursor=ValueCursor {phase:0,index:0};let mut facts=ValueFacts {collision:0,mutable:0,arrays:0,uniform:u32::from(value.uniform),control:u32::from(self.control_uniform),slots:0,regs:value.regs.len()};
  let mut value=Some(value);let mut owner=None;let mut slots=Vec::new();let mut storage=0..0;
  loop {let step=unsafe {value_step(cursor,facts)};cursor=step.cursor;match step.need {
   ValueNeed::View=>{self.require_view_available(value.as_ref().unwrap(),&[],span)?;facts.collision=u32::from(self.env.contains_key(name));},
   ValueNeed::Collision=>return self.fail(span,"B125",format!("重复局部变量或覆盖参数 {name}")),
   ValueNeed::Mutability=>facts.mutable=u32::from(self.mutable_names.contains(name)),
   ValueNeed::Arrays=>facts.arrays=u32::from(value.as_ref().unwrap().ty.contains_arrays()),
   ValueNeed::Array=>{let materialized=self.materialize_array(value.take().unwrap(),None);facts.uniform=u32::from(materialized.uniform);owner=Some(materialized);},
   ValueNeed::ArrayFinish=>{let mut owner=owner.take().unwrap();owner.place=owner.place.read_only();owner.uniform=step.uniform!=0;self.env.insert(name.to_string(),Binding::Owned(owner));return Ok(());},
   ValueNeed::Immutable=>{let mut value=value.take().unwrap();value.literal=false;value.uniform=step.uniform!=0;self.env.insert(name.to_string(),Binding::Value(value));return Ok(());},
   ValueNeed::Slots=>{(slots,storage)=self.local_slots(&value.as_ref().unwrap().ty);facts.slots=slots.len();},
   ValueNeed::Store=>self.emit(InstKind::StoreLocal(slots[step.index],value.as_ref().unwrap().regs[step.index])),
   ValueNeed::Finish=>{let value=value.take().unwrap();let proof=bounds::Proof::capture(&value);let place=Place::Local(value.ty,slots,storage);self.initialize(&place);self.env.insert(name.to_string(),Binding::Owned(OwnedBinding {proof,place,uniform:step.uniform!=0,initialization:None}));return Ok(());},
  }}
 }
}
