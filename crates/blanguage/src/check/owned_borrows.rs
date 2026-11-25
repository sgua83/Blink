//! 拥有位置直接引用原资源树；静态、动态赋值和函数输出共用同一位置交接。
use super::*;
use returns::Returned;
use application::Ownership;
use owned_presence::Presence;
use std::{borrow::Cow, ffi::c_void, ptr};
pub(super) fn positions(position:&Position,into:&mut BTreeSet<Position>){match position{Position::Indexed(choices,_)=>for choice in choices.iter(){positions(choice,into);},position=>{into.insert(position.clone());}}}
fn storage_ref(model: &Returned) -> Option<&Place> {
    match model {
        Returned::Dataset(data) => data.place.as_ref(),
        Returned::OwnedValue(value) => value.place.as_ref(),
        _ => None,
    }
}
fn storage(model: &Returned) -> Option<Place> { storage_ref(model).cloned() }
fn at<'a>(model:&'a Returned,path:&[usize])->Option<&'a application::Data>{match model{Returned::Dataset(data)if path.is_empty()=>Some(data),Returned::OwnedValue(value)=>view::owned_data_at(value,path),_=>None}}
pub(super) fn each_data(model:&mut Returned,path:&mut Vec<usize>,apply:&mut impl FnMut(&mut application::Data,&[usize])){match model{Returned::Dataset(data)=>apply(data,path),Returned::OwnedValue(value)=>for(index,payload)in value.payloads.iter_mut().enumerate(){if let Some(payload)=payload{path.push(index);each_data(payload,path,apply);path.pop();}},_=>{}}}
pub(super) fn set_storage(model:&mut Returned,place:Place){match model{Returned::Dataset(data)=>data.place=Some(place),Returned::OwnedValue(value)=>value.place=Some(place),_=>unreachable!()}}
struct PlaceSource<'a> {
    position: Position,
    value: &'a Returned,
    selected: Option<(Presence, bounds::Condition)>,
}

struct PlaceJoin<'a, 'w> {
    builder: &'a Builder<'w>,
    root: &'a Position,
    sources: Vec<PlaceSource<'a>>,
    result: Option<Returned>,
}

#[repr(C)]
struct PlaceChoiceHost {
    condition: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    merge: unsafe extern "C" fn(*mut c_void, u64, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_payload::join_place_candidates"]
    fn source_join_place_candidates(context: *mut c_void, count: u64, host: PlaceChoiceHost);
}

unsafe extern "C" fn place_condition(context: *mut c_void, at: u64) -> u32 {
    let context = unsafe { &mut *context.cast::<PlaceJoin<'_, '_>>() };
    let source = &mut context.sources[at as usize];
    let when = Presence::selected(context.root, &source.position);
    let condition = when.condition();
    let possible = context.builder.condition_truth(&condition, None) != Some(false);
    source.selected = possible.then_some((when, condition));
    u32::from(possible)
}

unsafe extern "C" fn merge_place_payload(context: *mut c_void, at: u64, have: u32) {
    let context = unsafe { &mut *context.cast::<PlaceJoin<'_, '_>>() };
    let source = context.sources[at as usize].value.clone();
    if have == 0 {
        context.result = Some(source);
    } else {
        context.result.as_mut().unwrap().join_place_payload(source);
    }
}

impl Builder<'_>{
    pub(super) fn owned_place_model(&self, place: &Place) -> Returned {
        self.join_owned_place_candidates(place, &self.owned_place_candidates(place))
    }

    pub(super) fn join_owned_place_candidates(&self, place: &Place, candidates: &[(String, Vec<usize>, Returned)]) -> Returned {
        fn initialization(position: &Position, path: &[usize], sources: &[PlaceSource<'_>]) -> Position {
            match position {
                Position::Indexed(choices, index) => Position::Indexed(
                    choices.iter().map(|choice| initialization(choice, path, sources)).collect(), *index,
                ),
                position => {
                    let source = sources.iter().find(|source| source.position == *position).unwrap();
                    at(source.value, path).and_then(|data| data.initialization.clone()).unwrap_or(Position::Initialized)
                }
            }
        }

        let root = place.owned_positions().unwrap().into_iter().next().unwrap();
        let mut join = PlaceJoin {
            builder: self,
            root: &root,
            result: None,
            sources: candidates.iter().map(|(_, _, value)| PlaceSource {
                position: storage_ref(value).unwrap().owned_positions().unwrap().into_iter().next().unwrap(),
                value,
                selected: None,
            }).collect(),
        };
        let count = join.sources.len() as u64;
        let host = PlaceChoiceHost { condition: place_condition, merge: merge_place_payload };
        unsafe { source_join_place_candidates((&mut join as *mut PlaceJoin<'_, '_>).cast(), count, host) };
        let mut model = join.result.take().unwrap();
        model.select_place_presence(self, join.sources.iter().filter_map(|source| {
            source.selected.as_ref().map(|(when, condition)| (when, condition, source.value))
        }));
        each_data(&mut model, &mut Vec::new(), &mut |data, path| {
            data.initialization = Some(initialization(&root, path, &join.sources));
        });
        set_storage(&mut model, place.clone());
        self.resolve_owned_model(&mut model, None);
        model
    }
    pub(super) async fn mutable_argument(&mut self,expr:&Expr,access:Access)->Result<Option<cooperative::Argument>> {
        let mut cursor=MutableCursor {phase:0};let mut facts=MutableInputFacts::default();
        let mut data=None;let mut place=None;let mut result=None;
        loop {let step=unsafe{mutable_input_step(cursor,facts)};cursor=step.cursor;match step.need {
            MutableInputNeed::Reference=>{data=unsafe{self.world.call(self.application_reference(expr)).await}?;facts.present=u32::from(data.is_some());},
            MutableInputNeed::Borrowed=>return Ok(Some(cooperative::Argument::BorrowedData(data.take().unwrap()))),
            MutableInputNeed::IsPlace=>facts.place=u32::from(self.is_place(expr)),
            MutableInputNeed::Place=>{place=unsafe{self.world.call(self.mutable_place(expr,access)).await}?;facts.present=u32::from(place.is_some());},
            MutableInputNeed::Dataset=>facts.dataset=u32::from(place.as_ref().unwrap().ty().dataset.is_some()),
            MutableInputNeed::Application=>{let data=unsafe{self.world.call(self.application_place_reference(place.take().unwrap(),expr.span())).await}?;return Ok(Some(cooperative::Argument::BorrowedData(data)));},
            MutableInputNeed::Owns=>facts.owns=u32::from(place.as_ref().unwrap().ty().owns_resources()),
            MutableInputNeed::Plain=>return Ok(Some(cooperative::Argument::Reference(self.borrow_owned(place.take().unwrap())))),
            MutableInputNeed::Owned=>return unsafe{self.world.call(self.owned_reference_argument(place.take().unwrap(),None,access,expr.span())).await},
            MutableInputNeed::Evaluate=>{result=unsafe{self.world.call(self.evaluate_returned(expr,None)).await}?;facts.continuing=u32::from(self.flow.continues());},
            MutableInputNeed::Returned=>return unsafe{self.world.call(self.mutable_returned_argument(result,access,expr.span())).await},
            MutableInputNeed::Empty=>return Ok(None),
        }}
    }
    pub(super) async fn mutable_returned_argument(&mut self,mut result:Option<Returned>,access:Access,span:Span)->Result<Option<cooperative::Argument>> {
        let mut cursor=MutableCursor {phase:0};let kind=result.as_ref().map_or(0,Returned::kind);
        let mut owner=None;let mut value=None;let mut place=None;
        loop {let step=unsafe{mutable_value_step(cursor,kind)};cursor=step.cursor;match step.need {
            MutableValueNeed::HoldData=>{let Returned::Dataset(data)=result.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};owner=Some(self.hold_binding(Binding::ApplicationData(data.clone())));},
            MutableValueNeed::BorrowData=>{let Returned::Dataset(data)=result.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};data.ownership=Ownership::Borrowed;},
            MutableValueNeed::Scoped=>return Ok(Some(cooperative::Argument::Scoped{value:result.take().unwrap(),owner:owner.take().unwrap()})),
            MutableValueNeed::BindOwner=>{let Returned::OwnedValue(item)=result.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};value=Some(self.owned_value_binding(item,true));},
            MutableValueNeed::OwnerPlace=>place=value.as_ref().unwrap().place(),
            MutableValueNeed::HoldOwner=>owner=Some(self.hold_binding(Binding::OwnedValue(value.take().unwrap()))),
            MutableValueNeed::Reference=>return unsafe{self.world.call(self.owned_reference_argument(place.take().unwrap(),owner,access,span)).await},
            MutableValueNeed::Invalid=>return self.fail(span,"B224","独占临时参数需要实际拥有值或dataset"),
        }}
    }
    async fn owned_reference_argument(&mut self,place:Place,owner:Option<String>,access:Access,span:Span)->Result<Option<cooperative::Argument>> {
        let mut cursor=MutableCursor {phase:0};let mut facts=OwnedInputFacts {application:u32::from(matches!(place.storage(),Place::ApplicationElement{..})),borrowed:0,available:0,access};
        let mut value=None;
        loop {let step=unsafe{owned_input_step(cursor,facts)};cursor=step.cursor;match step.need {
            need @ (OwnedInputNeed::Application|OwnedInputNeed::Model)=>{
                let model=match need{OwnedInputNeed::Application=>unsafe{self.world.call(self.application_owned_argument(&place,step.reading!=0,span)).await}?,_=>self.owned_place_model(&place)};
                let Returned::OwnedValue(item)=model else{unsafe{std::hint::unreachable_unchecked()}};facts.borrowed=u32::from(item.ownership==Ownership::Borrowed);value=Some(item);
            }
            OwnedInputNeed::Borrowed=>return self.fail(span,"B227","read聚合不能提升为独占借用"),
            OwnedInputNeed::Prepare=>self.prepare_owned_value(value.as_ref().unwrap()),
            OwnedInputNeed::Available=>facts.available=u32::from(value.as_ref().unwrap().all_available()),
            OwnedInputNeed::Unavailable=>return self.fail(span,"B227","inout聚合需要全部活动拥有资源"),
            OwnedInputNeed::Load=>unsafe{self.world.call(self.load_owned_value(value.as_mut().unwrap(),span)).await}?,
            OwnedInputNeed::Reference=>return Ok(Some(cooperative::Argument::OwnedReference{value:value.take().unwrap(),place,owner})),
        }}
    }
    pub(super) async fn complete_task_owned_places(
        &mut self,sources:&view::Borrow,
        returned:impl Fn(&mut Self,&outputs::TaskGuard)->Option<Value>,success:Option<&Value>,
        complete_inputs:impl std::ops::AsyncFn(&mut Self,&[view::TaskInputCompletion])->Result<()>,span:Span,
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
            let complete=async |builder:&mut Self| {
                builder.path=bounds::Clause::both(&builder.path,when);
                let complete_guards=async |builder:&mut Self| {
                    for guard in guards {
                        let target=address::Initialization{
                            source:address::InitializationSource::Task(guard.target.clone()),
                            when:bounds::Condition::constant(true),
                        }.positions(&builder.path);
                        if !guard.ty.owns_resources() {
                            let returned=returned(builder,guard);
                            let guards=unsafe { builder.world.call(guard.guards(builder,returned.as_ref(),span,Some)).await }?;
                            builder.initialize_output_positions(target,&guards);
                        } else if let Some(reference) = builder.task_dataset_output_reference(&guard.target, &guard.ty) {
                            let returned = returned(builder, guard);
                            let guards = unsafe { builder.world.call(guard.guards(builder, returned.as_ref(), span, Some)).await }?;
                            let mut model = unsafe { builder.world.call(guard.written_value(builder, returned.as_ref())).await }?;
                            outputs::apply_ownership(&mut model, &guard.ty, 0, &guards);
                            unsafe { builder.world.call(builder.apply_application_dataset_model(&reference, application::ElementFacts::Known(Box::new(model)))).await }?;
                            builder.initialize_output_positions(target, &guards);
                        } else if let Some(place)=owners.iter().find_map(|owner|find(builder,owner,&target)) {
                            let returned=returned(builder,guard);
                            let guards=unsafe { builder.world.call(guard.guards(builder,returned.as_ref(),span,Some)).await }?;
                            let mut model=unsafe { builder.world.call(guard.written_value(builder,returned.as_ref())).await }?;
                            outputs::apply_ownership(&mut model,place.ty(),0,&guards);
                            let proof=bounds::Proof::capture(&builder.returned_storage(&model));
                            unsafe { builder.world.call(builder.assign_owned_proof(&place,proof,false)).await }?;
                            unsafe { builder.world.call(builder.assign_owned_place(&place,model)).await }?;
                            builder.initialize_output(&place,&guards);
                        }
                    }
                    Ok(())
                };
                if !guards.is_empty() {
                    if let Some(success)=success {
                        unsafe { builder.world.call(builder.conditional_blocks(success.clone(),complete_guards,async |_|Ok(()),span)).await }?;
                    } else {
                        complete_guards(builder).await?;
                    }
                }
                complete_inputs(builder,completions).await
            };
            if closed && active.len()==1 {
                complete(self).await?;
                continue;
            }
            if closed && let Some(presence) = self.task_source_presence(source_kind, &active) {
                let condition = Value {
                    ty: Ty::scalar(Scalar::Bool),
                    regs: vec![self.presence_register(&presence, None)],
                    proof: bounds::Proof::condition(presence.condition()),
                    literal: false,
                    uniform: false,
                    constant: None,
                };
                unsafe { self.world.call(self.conditional_blocks(condition,complete,async |_|Ok(()),span)).await }?;
            }
        }
        Ok(())
    }
    fn task_source_controls<'a>(&'a self, active: &[(&view::Source, bounds::Clause)]) -> BTreeMap<Reg, Cow<'a, Value>> {
        let task_sources=active.iter().filter_map(|(_,when)|when.task_controls().map(|controls|(when,controls)))
            .collect::<Vec<_>>();
        let task_controls=task_sources.first().map(|(_,controls)|controls.keys().copied()
            .filter(|reg|task_sources.iter().all(|(_,controls)|controls.contains_key(reg)))
            .collect::<Vec<_>>()).unwrap_or_default();
        let mut decisions=self.env.values().filter_map(|binding|match binding {
            Binding::Value(value) if value.ty==Ty::scalar(Scalar::Bool)=>Some((value.regs[0], Cow::Borrowed(value))),
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
            decisions.insert(reg, Cow::Owned(Value {
                ty:Ty::scalar(Scalar::Bool),regs:vec![reg],proof,
                literal:false,uniform:false,constant:None,
            }));
        }
        decisions
    }
    pub(super) fn task_source_presence(
        &self, source: &view::Source, active: &[(&view::Source, bounds::Clause)],
    ) -> Option<Presence> {
        let mut search = TaskSourceSearch::new(self, source, active);
        let mut result = Presence::None;
        let selected = unsafe {
            task_source_select(ptr::from_mut(&mut search).cast(), active.len(),
                ptr::from_mut(&mut result).cast(), TASK_SOURCE_HOST)
        };
        (selected != 0).then_some(result)
    }

    pub(super) async fn assign_owned_place(&mut self,place:&Place,value:Returned)->Result<()>{
        if matches!(place.storage(),Place::ApplicationElement{..}) {
            return unsafe { self.world.call(self.assign_application_owned_argument(place, Some(&value))).await };
        }
        for(name,path,old)in self.owned_place_candidates(place){
            let mut value=value.clone();
            if let(Returned::OwnedValue(value),Returned::OwnedValue(old))=(&mut value,&old){value.declaration=old.declaration;}
            let actual=storage(&old).unwrap();let when=Presence::selected(&place.owned_positions().unwrap()[0],&actual.owned_positions().unwrap()[0]);value.join_presence(old,&when,None);set_storage(&mut value,actual);
            self.store_owned_candidate(name,&path,value);
        }
        Ok(())
    }
    fn store_owned_candidate(&mut self,name:String,path:&[usize],value:Returned){
        fn replace(owner:&mut owned_values::OwnedValue,path:&[usize],value:Returned){let target=owner.payloads[path[0]].as_deref_mut().unwrap();if path.len()==1{*target=value;}else{let Returned::OwnedValue(child)=target else{unreachable!()};replace(child,&path[1..],value);}}
        if path.is_empty(){self.env.insert(name,match value{Returned::Dataset(data)=>Binding::ApplicationData(data),Returned::OwnedValue(value)=>Binding::OwnedValue(value),_=>unreachable!()});}else{let Some(Binding::OwnedValue(owner))=self.env.get_mut(&name)else{unreachable!()};replace(owner,path,value);}
    }
    pub(super) async fn move_owned_place(&mut self, place: &Place) -> Result<()> {
        if matches!(place.storage(), Place::ApplicationElement { .. }) {
            return unsafe { self.world.call(self.assign_application_owned_argument(place, None)).await };
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
    pub(super) async fn take_owned_place(&mut self,place:Place,span:Span)->Result<Returned>{
        unsafe { self.world.call(self.require_place_access(&place,Access::Write,span)).await }?;
        let element=matches!(place.storage(),Place::ApplicationElement{..});
        let mut value=if element{
            self.require_owned_writable(&place,span)?;unsafe { self.world.call(self.writable_storage(&place,span,None)).await }?;
            unsafe { self.world.call(self.application_owned_argument(&place,true,span)).await }?
        }else{self.owned_place_model(&place)};
        let resources=match &value{Returned::Dataset(data)=>vec![data],Returned::OwnedValue(value)=>value.resources(),_=>Vec::new()};
        for data in resources{if self.application_position_borrowed(data.root.identity()){return self.fail(span,"B227","dataset的元素仍被游标或位置借用，不能移动拥有值");}}
        match &value{Returned::Dataset(data)=>{self.application_available(data,span)?;if data.ownership!=Ownership::Owned{return self.fail(span,"B227","借用资源不能从拥有位置移动");}},Returned::OwnedValue(value)=>{if !value.all_available(){return self.fail(span,"B227","拥有位置仍有未初始化或已经移动的资源");}},_=>unreachable!()}
        self.prepare_owned_read(&value);unsafe { self.world.call(self.require_initialized(&place,span)).await }?;
        let packed=if element{self.returned_storage(&value)}else{unsafe { self.world.call(self.read_storage_value(&place,span,None)).await }?};
        unsafe { self.world.call(self.move_owned_place(&place)).await }?;
        fn handoff(value:&mut Returned,old:&Returned){match(value,old){
            (Returned::Dataset(data),Returned::Dataset(old))=>{data.initialization=old.initialization.clone();let identity=ValueIdentity::Register(data.root.regs[0]);let mut borrow=old.root.proof.view_borrow(0);if borrow.empty(){borrow.sources.insert(view::Source::Data(old.root.identity()));}data.root.proof={let mut component_proof=bounds::Proof::default().with_identity(identity);component_proof.set_view_borrow(0,borrow);component_proof};data.length.proof=bounds::Proof::capture(&data.length).with_domains(Domains::from([IndexDomain::ApplicationLength(identity)]));},
            (Returned::OwnedValue(value),Returned::OwnedValue(old))=>for(index,payload)in value.payloads.iter_mut().enumerate(){if let Some(payload)=payload{handoff(payload,old.payloads[index].as_deref().unwrap());}},_=>{}}}
        let old=value.clone();native_function::rebind_model(&mut value,packed.regs);handoff(&mut value,&old);Ok(value)
    }
    pub(super) async fn assign_owned_storage(&mut self,place:Place,op:&str,rhs:&Expr,span:Span)->Result<()> {
        self.require_owned_writable(&place,span)?;
        unsafe { self.world.call(self.writable_storage(&place,span,Some(bounds::Proof::default()))).await }?;
        if op!="="{return self.fail(span,"B224","拥有内容只支持整份移动赋值");}
        let held = matches!(place.storage(), Place::ApplicationElement { .. })
            .then(|| self.hold_binding(Binding::AtomicBorrow(place.clone())));
        let result=unsafe { self.world.call(self.evaluate_returned(rhs,Some(place.ty()))).await };
        if let Some(held)=held{self.env.remove(&held);}
        let result=result?;
        if !self.flow.continues(){return Ok(());}
        let value=result.ok_or_else(||Diagnostic{source:self.scope.source().clone(),span,code:"B228",message:"拥有赋值需要实际值或资源".into()})?;
        self.view_uses.after(rhs.span());
        let old=if matches!(place.storage(),Place::ApplicationElement{..}) {
            unsafe { self.world.call(self.application_owned_argument(&place,false,span)).await }?
        } else {
            self.owned_place_model(&place)
        };
        unsafe { self.world.call(self.store_owned_returned(place,old,value,span)).await }
    }
    pub(super) async fn store_owned_returned(&mut self,place:Place,old:Returned,mut value:Returned,span:Span)->Result<()> {
        unsafe { self.world.call(self.check_returned_type(&mut value,place.ty(),span)).await }?;
        let packed=self.returned_storage(&value);
        let held=self.hold_binding(match &value {
            Returned::Dataset(data)=>Binding::ApplicationData(data.clone()),
            Returned::OwnedValue(owner)=>Binding::OwnedValue(owner.clone()),
            _=>unreachable!(),
        });
        unsafe { self.world.call(self.move_owned_place(&place)).await }?;
        unsafe { self.world.call(self.drop_returned(old,&view::Borrow::default(),span)).await }?;
        unsafe { self.world.call(self.write_storage_value(&place,&packed,span,None)).await }?;
        unsafe { self.world.call(self.assign_owned_proof(&place,bounds::Proof::capture(&packed),packed.uniform)).await }?;
        self.initialize(&place);
        unsafe { self.world.call(self.assign_owned_place(&place,value)).await }?;
        self.env.remove(&held);
        Ok(())
    }
}

#[repr(C)]#[derive(Clone,Copy)]struct MutableCursor {phase:u32}
#[repr(C)]#[derive(Clone,Copy,Default)]struct MutableInputFacts {present:u32,place:u32,dataset:u32,owns:u32,continuing:u32}
#[repr(u32)]enum MutableInputNeed {Reference,Borrowed,IsPlace,Place,Dataset,Application,Owns,Plain,Owned,Evaluate,Returned,Empty}
#[repr(C)]struct MutableInputStep {cursor:MutableCursor,need:MutableInputNeed}
#[repr(u32)]enum MutableValueNeed {HoldData,BorrowData,Scoped,BindOwner,OwnerPlace,HoldOwner,Reference,Invalid}
#[repr(C)]struct MutableValueStep {cursor:MutableCursor,need:MutableValueNeed}
#[repr(C)]#[derive(Clone,Copy)]struct OwnedInputFacts {application:u32,borrowed:u32,available:u32,access:Access}
#[repr(u32)]enum OwnedInputNeed {Application,Model,Borrowed,Prepare,Available,Unavailable,Load,Reference}
#[repr(C)]struct OwnedInputStep {cursor:MutableCursor,need:OwnedInputNeed,reading:u32}
unsafe extern "C" {
 #[link_name="bootstrap.call_arguments::mutable_input_step"]fn mutable_input_step(cursor:MutableCursor,facts:MutableInputFacts)->MutableInputStep;
 #[link_name="bootstrap.call_arguments::mutable_value_step"]fn mutable_value_step(cursor:MutableCursor,kind:u32)->MutableValueStep;
 #[link_name="bootstrap.call_arguments::owned_input_step"]fn owned_input_step(cursor:MutableCursor,facts:OwnedInputFacts)->OwnedInputStep;
}

type Address = *mut c_void;
type SourceControls<'a> = std::collections::btree_map::IntoValues<Reg, Cow<'a, Value>>;

struct TaskSourceSearch<'a, 'world> {
    builder: &'a Builder<'world>,
    source: &'a view::Source,
    active: &'a [(&'a view::Source, bounds::Clause)],
    controls: Option<SourceControls<'a>>,
    current: Option<Cow<'a, Value>>,
}

impl<'a, 'world> TaskSourceSearch<'a, 'world> {
    fn new(builder: &'a Builder<'world>, source: &'a view::Source,
        active: &'a [(&'a view::Source, bounds::Clause)]) -> Self {
        Self { builder, source, active, controls: None, current: None }
    }
}

#[repr(C)]
struct TaskSourceBranches {
    yes: Address,
    yes_count: usize,
    yes_result: Address,
    no: Address,
    no_count: usize,
    no_result: Address,
}

#[repr(C)]
struct TaskSourceHost {
    matches: unsafe extern "C" fn(Address, usize) -> u32,
    next_control: unsafe extern "C" fn(Address) -> *const c_void,
    partition: unsafe extern "C" fn(Address, *const c_void, Address) -> u32,
    truth: unsafe extern "C" fn(Address, u32),
    merge: unsafe extern "C" fn(Address, *const c_void, Address, Address),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_presence::task_source_select"]
    fn task_source_select(input: Address, count: usize, output: Address, host: TaskSourceHost) -> u32;
    #[link_name = "bootstrap.ownership_presence::task_source_split"]
    fn task_source_split(count: usize, control: *const c_void, branches: TaskSourceBranches,
        output: Address, host: TaskSourceHost) -> u32;
}

unsafe extern "C" fn source_matches(input: Address, index: usize) -> u32 {
    let search = unsafe { &*input.cast::<TaskSourceSearch<'_, '_>>() };
    u32::from(search.active[index].0 == search.source)
}

unsafe extern "C" fn next_control(input: Address) -> *const c_void {
    let search = unsafe { &mut *input.cast::<TaskSourceSearch<'_, '_>>() };
    let controls = search.controls.get_or_insert_with(||
        search.builder.task_source_controls(search.active).into_values());
    search.current = controls.next();
    search.current.as_deref().map_or(ptr::null(), |value| ptr::from_ref(value).cast())
}

// Both branch containers and the selected control live until this synchronous B call returns.
unsafe extern "C" fn partition_sources(input: Address, control: *const c_void, output: Address) -> u32 {
    let search = unsafe { &*input.cast::<TaskSourceSearch<'_, '_>>() };
    let value = unsafe { &*control.cast::<Value>() };
    let condition = bounds::Condition::of(value);
    let select = |truth| search.active.iter().filter_map(|(source, when)| {
        let when = condition.assume(when, truth);
        when.possible().then_some((*source, when))
    }).collect::<Vec<_>>();
    let yes = select(true);
    let no = select(false);
    let mut yes_search = TaskSourceSearch::new(search.builder, search.source, &yes);
    let mut no_search = TaskSourceSearch::new(search.builder, search.source, &no);
    let mut yes_result = Presence::None;
    let mut no_result = Presence::None;
    let branches = TaskSourceBranches {
        yes: ptr::from_mut(&mut yes_search).cast(),
        yes_count: yes.len(),
        yes_result: ptr::from_mut(&mut yes_result).cast(),
        no: ptr::from_mut(&mut no_search).cast(),
        no_count: no.len(),
        no_result: ptr::from_mut(&mut no_result).cast(),
    };
    unsafe { task_source_split(search.active.len(), control, branches, output, TASK_SOURCE_HOST) }
}

unsafe extern "C" fn task_source_truth(output: Address, value: u32) {
    unsafe { *output.cast::<Presence>() = if value == 0 { Presence::None } else { Presence::All }; }
}

unsafe extern "C" fn merge_task_source(output: Address, control: *const c_void, yes: Address, no: Address) {
    let condition = Presence::input(unsafe { &*control.cast::<Value>() });
    let yes = std::mem::replace(unsafe { &mut *yes.cast::<Presence>() }, Presence::None);
    let no = std::mem::replace(unsafe { &mut *no.cast::<Presence>() }, Presence::None);
    unsafe { *output.cast::<Presence>() = condition.clone().and(yes).or(condition.not().and(no)); }
}

const TASK_SOURCE_HOST: TaskSourceHost = TaskSourceHost {
    matches: source_matches,
    next_control,
    partition: partition_sources,
    truth: task_source_truth,
    merge: merge_task_source,
};
