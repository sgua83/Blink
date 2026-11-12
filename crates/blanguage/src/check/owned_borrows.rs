//! 拥有位置直接引用原资源树；静态、动态赋值和函数输出共用同一位置交接。
use super::*;
use returns::Returned;
use application::Ownership;
use owned_presence::Presence;
pub(super) fn positions(position:&Position,into:&mut BTreeSet<Position>){match position{Position::Indexed(choices,_)=>for choice in choices.iter(){positions(choice,into);},position=>{into.insert(position.clone());}}}
fn storage(model:&Returned)->Option<Place>{match model{Returned::Dataset(data)=>data.place.clone(),Returned::OwnedValue(value)=>value.place(),_=>None}}
fn at<'a>(model:&'a Returned,path:&[usize])->Option<&'a application::Data>{match model{Returned::Dataset(data)if path.is_empty()=>Some(data),Returned::OwnedValue(value)=>view::owned_data_at(value,path),_=>None}}
pub(super) fn each_data(model:&mut Returned,path:&mut Vec<usize>,apply:&mut impl FnMut(&mut application::Data,&[usize])){match model{Returned::Dataset(data)=>apply(data,path),Returned::OwnedValue(value)=>for(index,payload)in value.payloads.iter_mut().enumerate(){if let Some(payload)=payload{path.push(index);each_data(payload,path,apply);path.pop();}},_=>{}}}
pub(super) fn set_storage(model:&mut Returned,place:Place){match model{Returned::Dataset(data)=>data.place=Some(place),Returned::OwnedValue(value)=>value.place=Some(place),_=>unreachable!()}}
impl Builder<'_>{
    pub(super) fn owned_place_candidates(&mut self,place:&Place)->Vec<(String,Vec<usize>,Returned)> {
        fn collect(builder:&mut Builder<'_>,model:Returned,path:&mut Vec<usize>,wanted:&Place,selected:&BTreeSet<Position>,into:&mut Vec<(Vec<usize>,Returned)>){
            if storage(&model).is_some_and(|place|place.ty()==wanted.ty()&&selected.contains(&place.owned_positions().unwrap()[0])){into.push((path.clone(),model));return;}
            let Returned::OwnedValue(value)=model else{return};if value.value.ty.has_variants(){return;}
            for index in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(index).1.owns_resources(){path.push(index);let child=builder.unpack_record(&value,index);collect(builder,child,path,wanted,selected,into);path.pop();}}
        }
        let mut selected=BTreeSet::new();positions(&place.owned_positions().unwrap()[0],&mut selected);
        let roots=self.env.iter().filter_map(|(name,binding)|match binding{Binding::OwnedValue(value)=>Some((name.clone(),Returned::OwnedValue(value.clone()))),Binding::ApplicationData(data)if data.place.is_some()=>Some((name.clone(),Returned::Dataset(data.clone()))),_=>None}).collect::<Vec<_>>();
        let mut candidates=Vec::new();for(name,root)in roots{let mut found=Vec::new();collect(self,root,&mut Vec::new(),place,&selected,&mut found);candidates.extend(found.into_iter().map(|(path,value)|(name.clone(),path,value)));}candidates
    }
    pub(super) fn owned_place_model(&mut self,place:&Place)->Returned {
        fn initialization(position:&Position,path:&[usize],candidates:&[(String,Vec<usize>,Returned)])->Position{match position{
            Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|choice|initialization(choice,path,candidates)).collect(),*index),
            position=>{let model=&candidates.iter().find(|(_,_,model)|storage(model).unwrap().owned_positions().unwrap()[0]==*position).unwrap().2;at(model,path).and_then(|data|data.initialization.clone()).unwrap_or(Position::Initialized)}
        }}
        let candidates=self.owned_place_candidates(place);let root=place.owned_positions().unwrap()[0].clone();
        let selected=candidates.iter().map(|(_,_,value)|{let when=Presence::selected(&root,&storage(value).unwrap().owned_positions().unwrap()[0]);let mut value=value.clone();self.resolve_owned_model(&mut value,Some(&when));(when,value)}).filter(|(when,_)|self.owned_possible(when)).collect::<Vec<_>>();let mut values=selected.iter().map(|(_,value)|value.clone());let mut model=values.next().unwrap();for other in values{model.join(other,false,None);}model.select_presence(&selected);self.resolve_owned_model(&mut model,None);
each_data(&mut model,&mut Vec::new(),&mut|data,path|data.initialization=Some(initialization(&root,path,&candidates)));set_storage(&mut model,place.clone());self.resolve_owned_model(&mut model,None);model
    }
    pub(super) fn mutable_argument(&mut self,expr:&Expr,access:Access)->Result<Option<cooperative::Argument>> {
        if let Some(data)=self.application_reference(expr)?{
            return Ok(Some(cooperative::Argument::BorrowedData(data)));
        }
        let(place,owner)=if self.is_place(expr){
            let Some(place)=self.mutable_place(expr,access)?else{return Ok(None)};
            if place.ty().dataset.is_some(){
                let data=self.application_place_reference(place,expr.span())?;
                return Ok(Some(cooperative::Argument::BorrowedData(data)));
            }
            if !place.ty().owns_resources(){return Ok(Some(cooperative::Argument::Reference(self.borrow_owned(place))));}
            (place,None)
        }else{
            let result=self.evaluate_returned(expr,None)?;if !self.flow.continues(){return Ok(None);}
            return self.mutable_returned_argument(result,access,expr.span());
        };
        self.owned_reference_argument(place,owner,access,expr.span())
    }
    pub(super) fn mutable_returned_argument(&mut self,result:Option<Returned>,access:Access,span:Span)->Result<Option<cooperative::Argument>> {
            let value=match result{
                Some(Returned::Dataset(mut data))=>{
                    let owner=self.hold_binding(Binding::ApplicationData(data.clone()));
                    data.ownership=Ownership::Borrowed;
                    return Ok(Some(cooperative::Argument::Scoped{value:Returned::Dataset(data),owner}));
                },
                Some(Returned::OwnedValue(value))=>value,
                _=>return self.fail(span,"B224","独占临时参数需要实际拥有值或dataset"),
            };
            let value=self.owned_value_binding(value,true);
            let place=value.place().unwrap();
            let owner=self.hold_binding(Binding::OwnedValue(value));
        self.owned_reference_argument(place,Some(owner),access,span)
    }
    fn owned_reference_argument(&mut self,place:Place,owner:Option<String>,access:Access,span:Span)->Result<Option<cooperative::Argument>> {
        let model=if matches!(place.storage(),Place::ApplicationElement{..}){self.application_owned_argument(&place,access.reads(),span)?}else{self.owned_place_model(&place)};
        let Returned::OwnedValue(mut value)=model else{unreachable!()};
        if value.ownership==Ownership::Borrowed{return self.fail(span,"B227","read聚合不能提升为独占借用");}
        self.prepare_owned_read(&Returned::OwnedValue(value.clone()));
        if access==Access::InOut{if !value.all_available(){return self.fail(span,"B227","inout聚合需要全部活动拥有资源");}self.load_owned_value(&mut value,span)?;}
        Ok(Some(cooperative::Argument::OwnedReference{value,place,owner}))
    }
    pub(super) fn complete_task_owned_places(
        &mut self,sources:&view::Borrow,
        returned:impl Fn(&mut Self,&outputs::TaskGuard)->Option<Value>,success:Option<&Value>,
        complete_inputs:impl Fn(&mut Self,&[view::TaskInputCompletion])->Result<()>,span:Span,
    )->Result<()> {
        fn find(builder:&mut Builder<'_>,value:&owned_values::OwnedValue,target:&[Position])->Option<Place> {
            if let Some(place)=value.place() && builder.positions(&place)==target {return Some(place);}
            let ty=&value.value.ty;
            if ty.has_variants() || ty.has_owned_presence(){return None;}
            for index in 0..ty.aggregate_len() {
                if ty.aggregate_part(index).1.owns_resources()
                    && let Returned::OwnedValue(child)=builder.unpack_record(value,index)
                    && let Some(place)=find(builder,&child,target) {return Some(place);}
            }
            None
        }
        let sources=sources.restricted(&self.path);
        let active=sources.sources.entries().filter(|(_,when)|when.possible()).collect::<Vec<_>>();
        let closed=!sources.inputs.entries().any(|(_,when)|when.possible())
            && !sources.calls.entries().any(|(_,when)|when.possible())
            && !sources.reads.entries().any(|(_,when)|when.possible());
        let owners=self.env.values().filter_map(|binding|match binding {
            Binding::OwnedValue(value)=>Some(value.clone()),
            _=>None,
        }).collect::<Vec<_>>();
        for (source_kind,when) in &active {
            let view::Source::Task{guards,completions,..}=source_kind else{continue;};
            let complete=|builder:&mut Self| {
                builder.path=bounds::Clause::both(&builder.path,when);
                let complete_guards=|builder:&mut Self| {
                    for guard in guards {
                        let target=address::Initialization{
                            source:address::InitializationSource::Task(guard.target.clone()),
                            when:bounds::Condition::constant(true),
                        }.positions(&builder.path);
                        if !guard.ty.owns_resources() {
                            let returned=returned(builder,guard);
                            let guards=guard.guards(builder,returned.as_ref(),span)?;
                            builder.initialize_output_positions(target,&guards);
                        } else if let Some(reference) = builder.task_dataset_output_reference(&guard.target, &guard.ty) {
                            let returned = returned(builder, guard);
                            let guards = guard.guards(builder, returned.as_ref(), span)?;
                            let mut model = guard.written_value(builder, returned.as_ref())?;
                            outputs::apply_ownership(&mut model, &guard.ty, 0, &guards);
                            builder.apply_application_dataset_model(&reference, application::ElementFacts::Known(Box::new(model)))?;
                            builder.initialize_output_positions(target, &guards);
                        } else if let Some(place)=owners.iter().find_map(|owner|find(builder,owner,&target)) {
                            let returned=returned(builder,guard);
                            let guards=guard.guards(builder,returned.as_ref(),span)?;
                            let mut model=guard.written_value(builder,returned.as_ref())?;
                            outputs::apply_ownership(&mut model,place.ty(),0,&guards);
                            let proof=bounds::Proof::capture(&builder.returned_storage(&model));
                            builder.assign_owned_proof(&place,proof,false)?;
                            builder.assign_owned_place(&place,model)?;
                            builder.initialize_output(&place,&guards);
                        }
                    }
                    Ok(())
                };
                if !guards.is_empty() {
                    if let Some(success)=success {
                        builder.conditional_blocks(success.clone(),complete_guards,|_|Ok(()),span)?;
                    } else {
                        complete_guards(builder)?;
                    }
                }
                complete_inputs(builder,completions)
            };
            if closed && active.len()==1 {
                complete(self)?;
                continue;
            }
            if closed && let Some(condition)=self.task_source_condition(source_kind,&active,span)? {
                self.conditional_blocks(condition,complete,|_|Ok(()),span)?;
            }
        }
        Ok(())
    }
    fn task_source_controls(&self,active:&[(&view::Source,bounds::Clause)])->BTreeMap<Reg,Value> {
        let task_sources=active.iter().filter_map(|(_,when)|when.task_controls().map(|controls|(when,controls)))
            .collect::<Vec<_>>();
        let task_controls=task_sources.first().map(|(_,controls)|controls.keys().copied()
            .filter(|reg|task_sources.iter().all(|(_,controls)|controls.contains_key(reg)))
            .collect::<Vec<_>>()).unwrap_or_default();
        let mut decisions=self.env.values().filter_map(|binding|match binding {
            Binding::Value(value) if value.ty==Ty::scalar(Scalar::Bool)=>Some((value.regs[0],value.clone())),
            _=>None,
        }).collect::<BTreeMap<_,_>>();
        for reg in task_controls {
            let mut cases=std::array::from_fn(|_|bounds::Clause::impossible());
            for (when,controls) in &task_sources {
                let index=usize::from(controls[&reg]);
                cases[index]=bounds::Clause::either(&cases[index],when);
            }
            let [no,yes]=cases;
            let proof=bounds::Proof::condition(bounds::Condition::paths(yes,no));
            decisions.insert(reg,Value {
                ty:Ty::scalar(Scalar::Bool),regs:vec![reg],proof,
                literal:false,uniform:false,constant:None,
            });
        }
        decisions
    }
    fn task_source_presence(
        &self, source:&view::Source, active:&[(&view::Source,bounds::Clause)],
    )->Option<Presence> {
        if active.iter().all(|(other,_)|*other!=source) {
            return Some(Presence::None);
        }
        if active.iter().all(|(other,_)|*other==source) {
            return Some(Presence::All);
        }
        // A nested source selector is available within the choices of its parent.
        let (decision,yes,no)=self.task_source_controls(active).into_values().find_map(|value| {
            let branch=bounds::Condition::of(&value);
            let select=|truth|active.iter().filter_map(|(source,when)| {
                let when=branch.assume(when,truth);
                when.possible().then_some((*source,when))
            }).collect::<Vec<_>>();
            let (yes,no)=(select(true),select(false));
            (yes.len()<active.len() && no.len()<active.len())
                .then_some((Presence::input(&value),yes,no))
        })?;
        let yes=self.task_source_presence(source,&yes)?;
        let no=self.task_source_presence(source,&no)?;
        Some(decision.clone().and(yes).or(decision.not().and(no)))
    }

    pub(super) fn task_source_condition(
        &mut self, source:&view::Source, active:&[(&view::Source,bounds::Clause)], span:Span,
    )->Result<Option<Value>> {
        let Some(presence)=self.task_source_presence(source,active) else { return Ok(None); };
        let reg=self.presence_register(&presence,span,None)?;
        let proof=bounds::Proof::condition(presence.condition());
        Ok(Some(Value{
            ty:Ty::scalar(Scalar::Bool),regs:vec![reg],proof,
            literal:false,uniform:false,constant:None,
        }))
    }
    pub(super) fn assign_owned_place(&mut self,place:&Place,value:Returned)->Result<()>{
        if matches!(place.storage(),Place::ApplicationElement{..}) {
            return self.assign_application_owned_argument(place, Some(&value));
        }
        for(name,path,old)in self.owned_place_candidates(place){
            let mut value=value.clone();
            if let(Returned::OwnedValue(value),Returned::OwnedValue(old))=(&mut value,&old){value.declaration=old.declaration;}
            let actual=storage(&old).unwrap();let when=Presence::selected(&place.owned_positions().unwrap()[0],&actual.owned_positions().unwrap()[0]);value.join_presence(old,&when);set_storage(&mut value,actual);
            self.store_owned_candidate(name,&path,value);
        }
        Ok(())
    }
    fn store_owned_candidate(&mut self,name:String,path:&[usize],value:Returned){
        fn replace(owner:&mut owned_values::OwnedValue,path:&[usize],value:Returned){let target=owner.payloads[path[0]].as_deref_mut().unwrap();if path.len()==1{*target=value;}else{let Returned::OwnedValue(child)=target else{unreachable!()};replace(child,&path[1..],value);}}
        if path.is_empty(){self.env.insert(name,match value{Returned::Dataset(data)=>Binding::ApplicationData(data),Returned::OwnedValue(value)=>Binding::OwnedValue(value),_=>unreachable!()});}else{let Some(Binding::OwnedValue(owner))=self.env.get_mut(&name)else{unreachable!()};replace(owner,path,value);}
    }
    pub(super) fn move_owned_place(&mut self, place: &Place) -> Result<()> {
        if matches!(place.storage(), Place::ApplicationElement { .. }) {
            return self.assign_application_owned_argument(place, None);
        }
        for (name, path, mut old) in self.owned_place_candidates(place) {
            let actual = storage(&old).unwrap();
            let when = Presence::selected(&place.owned_positions().unwrap()[0], &actual.owned_positions().unwrap()[0]);
            self.forget_owned_metadata(&old);
            old.restrict_owned(&when.not());
            self.store_owned_candidate(name, &path, old);
        }
        Ok(())
    }
    pub(super) fn take_owned_place(&mut self,place:Place,span:Span)->Result<Returned>{
        self.require_place_access(&place,Access::Write,span)?;
        let element=matches!(place.storage(),Place::ApplicationElement{..});
        let mut value=if element{
            self.require_owned_writable(&place,span)?;self.writable_storage(&place,span,None)?;
            self.application_owned_argument(&place,true,span)?
        }else{self.owned_place_model(&place)};
        let resources=match &value{Returned::Dataset(data)=>vec![data],Returned::OwnedValue(value)=>value.resources(),_=>Vec::new()};
        for data in resources{if self.application_position_borrowed(data.root.identity()){return self.fail(span,"B227","dataset的元素仍被游标或位置借用，不能移动拥有值");}}
        match &value{Returned::Dataset(data)=>{self.application_available(data,span)?;if data.ownership!=Ownership::Owned{return self.fail(span,"B227","借用资源不能从拥有位置移动");}},Returned::OwnedValue(value)=>{if !value.all_available(){return self.fail(span,"B227","拥有位置仍有未初始化或已经移动的资源");}},_=>unreachable!()}
        self.prepare_owned_read(&value);self.require_initialized(&place,span)?;
        let packed=if element{self.returned_storage(&value)}else{self.read_storage_value(&place,span,None)?};
        self.move_owned_place(&place)?;
        fn handoff(value:&mut Returned,old:&Returned){match(value,old){
            (Returned::Dataset(data),Returned::Dataset(old))=>{data.initialization=old.initialization.clone();let identity=ValueIdentity::Register(data.root.regs[0]);let mut borrow=old.root.proof.view_borrow(0);if borrow.empty(){borrow.sources.insert(view::Source::Data(old.root.identity()));}data.root.proof=bounds::Proof::default().with_identity(identity).with_view_borrow(0,borrow);data.length.proof=bounds::Proof::capture(&data.length).with_domains(Domains::from([IndexDomain::ApplicationLength(identity)]));},
            (Returned::OwnedValue(value),Returned::OwnedValue(old))=>for(index,payload)in value.payloads.iter_mut().enumerate(){if let Some(payload)=payload{handoff(payload,old.payloads[index].as_deref().unwrap());}},_=>{}}}
        let old=value.clone();native_function::rebind_model(&mut value,packed.regs);handoff(&mut value,&old);Ok(value)
    }
    pub(super) fn assign_owned_storage(&mut self,place:Place,reference:Option<application::DataReference>,op:&str,rhs:&Expr,span:Span)->Result<()> {
        self.require_owned_writable(&place,span)?;
        self.writable_storage(&place,span,Some(bounds::Proof::default()))?;
        if op!="="{return self.fail(span,"B224","拥有内容只支持整份移动赋值");}
        let held=reference.map(|data|self.hold_binding(Binding::DataReference(data)));
        let result=self.evaluate_returned(rhs,Some(place.ty()));
        if let Some(held)=held{self.env.remove(&held);}
        let result=result?;
        if !self.flow.continues(){return Ok(());}
        let value=result.ok_or_else(||Diagnostic{source:self.scope.source().clone(),span,code:"B228",message:"拥有赋值需要实际值或资源".into()})?;
        self.view_uses.after(rhs.span());
        let old=if matches!(place.storage(),Place::ApplicationElement{..}) {
            self.application_owned_argument(&place,false,span)?
        } else {
            self.owned_place_model(&place)
        };
        self.store_owned_returned(place,old,value,span)
    }
    pub(super) fn store_owned_returned(&mut self,place:Place,old:Returned,mut value:Returned,span:Span)->Result<()> {
        self.check_returned_type(&mut value,place.ty(),span)?;
        let packed=self.returned_storage(&value);
        let held=self.hold_binding(match &value {
            Returned::Dataset(data)=>Binding::ApplicationData(data.clone()),
            Returned::OwnedValue(owner)=>Binding::OwnedValue(owner.clone()),
            _=>unreachable!(),
        });
        self.move_owned_place(&place)?;
        self.drop_returned(old,&view::Borrow::default(),span)?;
        self.write_storage_value(&place,&packed,span,None)?;
        self.assign_owned_proof(&place,bounds::Proof::capture(&packed),packed.uniform)?;
        self.initialize(&place);
        self.assign_owned_place(&place,value)?;
        self.env.remove(&held);
        Ok(())
    }
}
