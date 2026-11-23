//! 选择式先检查两侧，在运行时只求值一侧；结果仍是普通值。
use super::*;
impl Builder<'_> {
    #[inline(never)]
    pub(super) async fn selection(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
    ) -> Result<Option<Value>> {
        unsafe { self.world.call(self.selection_expected(condition, yes, no, span, None)).await }
    }
    pub(super) async fn selection_expected(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
        expected: Option<&Ty>,
    ) -> Result<Option<Value>> {
        let result = unsafe { self.world.call(self.selection_returned(condition, yes, no, span, expected)).await }?;
        self.returned_expression(result, span, false)
    }
    #[inline(never)]
    pub(super) async fn selection_returned(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
        expected: Option<&Ty>,
    ) -> Result<Option<returns::Returned>> {
        let Some(c) = unsafe { self.world.call(self.temporary_scope(condition.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(condition)).await }?, view::Borrow::default())))).await }? else {
            return Ok(None);
        };
        self.require_bool(&c, condition.span())?;
        // 条件求值完成后才进入两臂合并，条件递归不保留合并阶段的局部状态。
        unsafe { self.world.call(self.selection_after_condition(c,
            (yes.span(), async |builder| unsafe { builder.world.call(builder.evaluate_returned(yes, expected)).await }),
            (no.span(), async |builder| unsafe { builder.world.call(builder.evaluate_returned(no, expected)).await }),
            async |builder, names| unsafe { builder.world.call(expression_assignments(builder, yes, names)).await }, span)).await }
    }
    pub(super) async fn selection_after_condition(
        &mut self,
        c: Value,
        yes: (Span, impl std::ops::AsyncFnOnce(&mut Self) -> Result<Option<returns::Returned>>),
        no: (Span, impl std::ops::AsyncFnOnce(&mut Self) -> Result<Option<returns::Returned>>),
        modified: impl std::ops::AsyncFnOnce(&Self, &mut BTreeSet<String>) -> Result<()>,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        let (yes_span,yes_body)=yes;let (no_span,no_body)=no;
        let mut yes_body=Some(yes_body);let mut no_body=Some(no_body);let mut modified=Some(modified);
        let mut cursor=SelectionCursor {phase:0};let mut facts=SelectionFacts {continuing:0,present:0,other:0,outer:0,condition:u32::from(c.uniform)};
        let mut entry:Option<SelectionEntry>=None;let mut first:Option<SelectionFirst>=None;let mut partition=None;
        let mut yes_modified=BTreeSet::new();let mut control=false;let mut first_exit=0;let mut yes_exit=0;
        let mut a_result=None;let mut yes_owners=Vec::new();let mut b_result=None;let mut no_owners=Vec::new();
        loop {
            let step=unsafe {selection_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                SelectionNeed::Prepare=>{
                    self.remember_loop_control(&c);partition=Some(Arc::new(bounds::Condition::of(&c)));
                    let prefix=self.code.len();let outer_regions=std::mem::take(&mut self.memory_regions);let environment=self.env.clone();
                    let original=environment.keys().cloned().collect::<BTreeSet<_>>();let path=self.path.clone();let initialized=self.application_initialization_state();
                    let effects=(self.reads.clone(),self.writes.clone(),self.cross_access.clone());
                    entry=Some(SelectionEntry {prefix,outer_regions,environment,original,path,initialized,effects});
                },
                SelectionNeed::Modified=>{modified.take().unwrap()(self,&mut yes_modified).await?;control=self.control_uniform;facts.outer=u32::from(control);},
                SelectionNeed::EnterYes=>{self.control_uniform=step.uniform!=0;self.refine_bounds(partition.as_ref().unwrap(),true);self.flow=Flow::NEXT;},
                SelectionNeed::Yes=>{
                    first_exit=self.return_context.as_ref().map_or(0,|context|context.exit_count());
                    (a_result,yes_owners)=unsafe {self.world.call(self.selection_arm(yes_span,&entry.as_ref().unwrap().original,yes_body.take().unwrap())).await}?;
                    yes_exit=self.return_context.as_ref().map_or(0,|context|context.exit_count());facts.present=u32::from(a_result.is_some());facts.continuing=u32::from(self.flow.continues());
                },
                SelectionNeed::Missing=>return self.fail(span,"B228","选择式需要值"),
                SelectionNeed::EnterNo=>{
                    let SelectionEntry {prefix,outer_regions,environment,original,path,initialized,effects}=entry.take().unwrap();
                    let yes_flow=self.flow;let yes_env=self.env.clone();let yes_path=self.path.clone();
                    let yes_effects=(std::mem::replace(&mut self.reads,effects.0),std::mem::replace(&mut self.writes,effects.1),std::mem::replace(&mut self.cross_access,effects.2));
                    let yes_init=std::mem::replace(&mut self.initialized,initialized);let yes_init=self.application_empty_initialization(yes_init);
                    let yes=self.code.split_off(prefix);let yes_regions=std::mem::take(&mut self.memory_regions);
                    self.restore_expression_environment(environment,&yes_modified);self.path=path;self.refine_bounds(partition.as_ref().unwrap(),false);self.flow=Flow::NEXT;
                    first=Some(SelectionFirst {prefix,outer_regions,original,yes_flow,yes_env,yes_path,yes_init,yes_effects,yes,yes_regions,first_exit,yes_exit,a_result:a_result.take(),yes_owners:std::mem::take(&mut yes_owners)});
                },
                SelectionNeed::No=>{
                    (b_result,no_owners)=unsafe {self.world.call(self.selection_arm(no_span,&first.as_ref().unwrap().original,no_body.take().unwrap())).await}?;
                    facts.present=u32::from(b_result.is_some());facts.other=u32::from(first.as_ref().unwrap().a_result.is_some());facts.continuing=u32::from(self.flow.continues());
                },
                SelectionNeed::Same=>{
                    let a=self.returned_storage(first.as_ref().unwrap().a_result.as_ref().unwrap());let b=self.returned_storage(b_result.as_ref().unwrap());self.require_same(&a.ty,&b.ty,span)?;
                },
                SelectionNeed::Finish=>{
                    let SelectionFirst {prefix,outer_regions,original:_,yes_flow,yes_env,yes_path,yes_init,yes_effects,yes,yes_regions,first_exit,yes_exit,a_result,yes_owners}=first.take().unwrap();
                    let partition=partition.take().unwrap();
                    let no_flow = self.flow;
                    let no_path = self.path.clone();
                    let no_init = self.take_application_initialization_state();
                    let input=SelectionMergeInput {truth:c.constant.as_ref().map_or(0,|bits|if bits[0]!=0{2}else{1}),yes_live:u32::from(yes_flow.continues()),no_live:u32::from(no_flow.continues()),
                        returns:u32::from(self.return_context.is_some()),uniform:u32::from(control)};
                    let mut merge=SelectionMerge {builder:self,condition:&c,prefix,first_exit,yes_exit,yes_flow,no_flow,
                        yes_env:Some(yes_env),no_env:None,yes_path:Some(yes_path),no_path:Some(no_path),yes_init:Some(yes_init),no_init:Some(no_init),selected_init:None,yes_effects:Some(yes_effects),
                        outer_regions:Some(outer_regions),regions:[Some(yes_regions),None],no:None,region_entries:None,region:None,binding:0,write_entries:None,write:None,
                        owners:[Some(yes_owners),Some(no_owners)],presences:[None,None],owner_entries:None,present:None,owner:None};
                    unsafe {selection_merge(input,SelectionMergeHost {context:&mut merge,next_region:merge_region,next_write:merge_write,owner_possible:merge_owner_possible,next_owner:merge_owner,action:merge_action})};
                    let no=merge.no.take().unwrap();drop(merge);
                    let input=SelectionResultInput {truth:c.constant.as_ref().map_or(0,|bits|if bits[0]!=0{2}else{1}),uniform:u32::from(c.uniform)};
                    let mut values=SelectionValues {builder:self,condition:c,partition,yes,no,yes_result:a_result,no_result:b_result,
                        a:None,b:None,slots:Vec::new(),regs:Vec::new(),selected:None,proof:None,output:None};
                    unsafe {selection_result(input,SelectionHost {context:&mut values,storage:selection_storage,layout:selection_layout,action:selection_action})};
                    return Ok(values.output);
                },
            }
        }

    }
}

impl Builder<'_> {
    async fn selection_arm(&mut self,span:Span,original:&BTreeSet<String>,lower:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<returns::Returned>>)->Result<(Option<returns::Returned>,Vec<Binding>)> {
        unsafe { self.world.call(self.temporary_scope(span,async |builder|{
            let value=lower(builder).await?;
            let retained=value.as_ref().and_then(returns::Returned::view_value).map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();
            let borrow=retained.owners().restricted(&builder.path);
            let names={
                let mut bindings=builder.env.iter();let mut current=None;let mut resources=None;let mut resource=None;
                let mut references:Option<view::Borrow>=None;let mut sources=None;let mut source=None;let mut names=Vec::new();
                let mut cursor=OwnerCursor {phase:0};let mut facts=OwnerFacts {name:OwnerName {data:std::ptr::null(),length:0},present:0,kind:0,answer:0};
                loop {
                    let step=unsafe {owner_step(cursor,facts)};cursor=step.cursor;
                    match step.need {
                        OwnerNeed::NextBinding=>{
                            references=None;resources=None;resource=None;sources=None;source=None;current=bindings.next();
                            facts.present=u32::from(current.is_some());facts.name=current.map_or(OwnerName {data:std::ptr::null(),length:0},|(name,_)|OwnerName {data:name.as_ptr(),length:name.len()});
                        },
                        OwnerNeed::Original=>facts.answer=u32::from(original.contains(current.unwrap().0)),
                        OwnerNeed::Kind=>facts.kind=match current.unwrap().1 {Binding::OwnedValue(_)=>1,Binding::ApplicationData(_)=>2,_=>0},
                        OwnerNeed::MayOwn=>facts.answer=u32::from(matches!(current.unwrap().1,Binding::OwnedValue(value)if value.ownership.may_own())||matches!(current.unwrap().1,Binding::ApplicationData(data)if data.ownership.may_own())),
                        OwnerNeed::Reference=>facts.answer=u32::from(matches!(current.unwrap().1,Binding::OwnedValue(value)if matches!(value.place(),Some(Place::Reference{..})))),
                        OwnerNeed::Borrow=>{
                            let binding=current.unwrap().1;references=Some(unsafe {builder.world.call(binding.view_borrow(builder)).await}?.owners());resources=Some(binding.application_resources().into_iter());
                        },
                        OwnerNeed::NextResource=>{resource=resources.as_mut().unwrap().next();facts.present=u32::from(resource.is_some());},
                        OwnerNeed::UnionResource=>references.as_mut().unwrap().union(&builder.owned_resource_borrow(resource.unwrap()).owners()),
                        OwnerNeed::Restrict=>references=Some(references.take().unwrap().restricted(&builder.path)),
                        OwnerNeed::BorrowPending=>facts.answer=u32::from(borrow.pending()),
                        OwnerNeed::ReferencesPending=>facts.answer=u32::from(references.as_ref().unwrap().pending()),
                        OwnerNeed::StartSources=>sources=Some(borrow.sources.entries()),
                        OwnerNeed::NextSource=>{source=sources.as_mut().unwrap().next();facts.present=u32::from(source.is_some());},
                        OwnerNeed::Overlap=>{let(source,when)=source.take().unwrap();facts.answer=u32::from(references.as_ref().unwrap().overlaps_source(source,&when));},
                        OwnerNeed::Keep=>names.push(current.unwrap().0.clone()),
                        OwnerNeed::Complete=>break names,
                    }
                }
            };
            let count=names.len();let mut moves=OwnerMoves {builder,names:Some(names),ordered:None,owners:Vec::new()};
            unsafe {move_owners(count,OwnerNamesHost {context:&mut moves,name:owner_name,exchange:exchange_owner_names,begin:begin_owner_moves,take:take_owner_move})};
            let owners=moves.owners;
            Ok(((value,owners),retained))
        })).await }
    }
}


#[repr(C)]
#[derive(Clone,Copy)]
struct BranchCursor {phase:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct BranchFacts {truth:u32,outer_control:u32,condition_uniform:u32,yes_live:u32,no_live:u32}
#[repr(u32)]
enum BranchNeed {KnownYes,KnownNo,DynamicYes,DynamicNo,OnlyYes,OnlyNo,Both,Neither,Complete}
#[repr(C)]
struct BranchStep {cursor:BranchCursor,need:BranchNeed,uniform:u32}
unsafe extern "C" {
    #[link_name="bootstrap.function_body::branch_step"]
    fn branch_step(cursor:BranchCursor,facts:BranchFacts)->BranchStep;
}
struct BranchEntry {environment:BTreeMap<String,Binding>,path:bounds::Clause,initialized:BTreeSet<Position>}
struct BranchExit {code:Vec<Inst>,initialized:BTreeSet<Position>,environment:BTreeMap<String,Binding>,path:bounds::Clause,flow:Flow}
impl Builder<'_> {
    pub(super) async fn conditional_blocks(&mut self,c:Value,
        yes_body:impl std::ops::AsyncFnOnce(&mut Self)->Result<()>,no_body:impl std::ops::AsyncFnOnce(&mut Self)->Result<()>,span:Span)->Result<()> {
        let partition=Arc::new(bounds::Condition::of(&c));
        let mut facts=BranchFacts {truth:match partition.boolean_constant(){None=>0,Some(false)=>1,Some(true)=>2},
            outer_control:u32::from(self.control_uniform),condition_uniform:u32::from(c.uniform),yes_live:0,no_live:0};
        let mut cursor=BranchCursor {phase:0};let mut yes_body=Some(yes_body);let mut no_body=Some(no_body);
        let mut entry=None;let mut original=None;let mut yes_exit=None;let mut no_exit=None;let mut prefix=0;
        loop {
            let step=unsafe {branch_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                BranchNeed::KnownYes=>{self.refine_bounds(&partition,true);yes_body.take().unwrap()(self).await?;},
                BranchNeed::KnownNo=>{self.refine_bounds(&partition,false);no_body.take().unwrap()(self).await?;},
                BranchNeed::DynamicYes=>{
                    self.remember_loop_control(&c);
                    let environment=self.env.clone();let path=self.path.clone();prefix=self.code.len();let initialized=self.application_initialization_state();
                    entry=Some(BranchEntry {environment,path,initialized});
                    self.control_uniform=step.uniform!=0;self.refine_bounds(&partition,true);yes_body.take().unwrap()(self).await?;
                },
                BranchNeed::DynamicNo=>{
                    let code=self.code.split_off(prefix);let initialized=self.take_application_initialization_state();let environment=self.env.clone();let path=self.path.clone();let flow=self.flow;
                    facts.yes_live=u32::from(flow.continues());yes_exit=Some(BranchExit {code,initialized,environment,path,flow});
                    let outer=entry.take().unwrap();self.flow=Flow::NEXT;self.initialized=outer.initialized;self.env=outer.environment.clone();self.path=outer.path;
                    original=Some(outer.environment);self.refine_bounds(&partition,false);no_body.take().unwrap()(self).await?;
                    let code=self.code.split_off(prefix);let initialized=self.take_application_initialization_state();let flow=self.flow;facts.no_live=u32::from(flow.continues());
                    let environment=self.env.clone();let path=self.path.clone();no_exit=Some(BranchExit {code,initialized,environment,path,flow});
                },
                need @ (BranchNeed::OnlyYes|BranchNeed::OnlyNo|BranchNeed::Both|BranchNeed::Neither)=>{
                    let yes=yes_exit.take().unwrap();let mut no=no_exit.take().unwrap();
                    match need {
                        BranchNeed::OnlyYes=>{self.path=yes.path;self.env=yes.environment;self.initialized=yes.initialized;},
                        BranchNeed::OnlyNo=>{self.path=no.path;self.env=no.environment;self.initialized=no.initialized;},
                        BranchNeed::Both=>{
                            self.path=bounds::Clause::either(&yes.path,&no.path);self.view_uses.after(span);self.env=yes.environment;
                            let when=owned_presence::Presence::input(&c);self.join_initialization(&when,yes.initialized,no.initialized);
                            self.join_environment_selected(&mut no.environment,&when,&when.condition());
                        },
                        _=>{self.path=bounds::Clause::impossible();self.env=original.take().unwrap();self.join_initialization(&owned_presence::Presence::input(&c),yes.initialized,no.initialized);},
                    }
                    self.flow=yes.flow.union(no.flow);self.control_uniform=step.uniform!=0;
                    self.emit(InstKind::If {condition:c.regs[0],yes:yes.code,no:no.code});
                },
                BranchNeed::Complete=>return Ok(()),
            }
        }
    }
}

#[repr(C)]
struct SelectionResultInput {truth:u32,uniform:u32}
#[repr(C)]
struct SelectionStorage {yes:u32,no:u32,yes_regs:usize,no_regs:usize,yes_uniform:u32,no_uniform:u32}
#[repr(C)]
struct SelectionLayout {slots:usize,components:usize}
#[repr(u32)]
enum SelectionAction {Known,Store,Emit,Load,BeginPaths,Aliases,Path,CommonProof,CaptureProof,Join,Take,Finish}
#[repr(C)]
struct SelectionHost<'a,'w> {
 context:*mut SelectionValues<'a,'w>,
 storage:unsafe extern "C" fn(*mut SelectionValues<'a,'w>)->SelectionStorage,
 layout:unsafe extern "C" fn(*mut SelectionValues<'a,'w>)->SelectionLayout,
 action:unsafe extern "C" fn(*mut SelectionValues<'a,'w>,SelectionAction,u32,usize),
}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::selection_result"] fn selection_result(input:SelectionResultInput,host:SelectionHost<'_,'_>);
}
struct SelectionValues<'a,'w> {
 builder:&'a mut Builder<'w>,condition:Value,partition:Arc<bounds::Condition>,
 yes:Vec<Inst>,no:Vec<Inst>,yes_result:Option<returns::Returned>,no_result:Option<returns::Returned>,
 a:Option<Value>,b:Option<Value>,slots:Vec<usize>,regs:Vec<Reg>,selected:Option<bounds::Clause>,proof:Option<bounds::Proof>,output:Option<returns::Returned>,
}
impl SelectionValues<'_,'_> {
 fn storage(&self)->&Value {self.a.as_ref().or(self.b.as_ref()).unwrap()}
}
unsafe extern "C" fn selection_storage(context:*mut SelectionValues<'_,'_>)->SelectionStorage {
 let v=unsafe {&mut *context};
 v.a=v.yes_result.as_ref().map(|value|v.builder.returned_storage(value));v.b=v.no_result.as_ref().map(|value|v.builder.returned_storage(value));
 SelectionStorage {yes:u32::from(v.a.is_some()),no:u32::from(v.b.is_some()),yes_regs:v.a.as_ref().map_or(0,|v|v.regs.len()),no_regs:v.b.as_ref().map_or(0,|v|v.regs.len()),
  yes_uniform:u32::from(v.a.as_ref().is_none_or(|v|v.uniform)),no_uniform:u32::from(v.b.as_ref().is_none_or(|v|v.uniform))}
}
unsafe extern "C" fn selection_layout(context:*mut SelectionValues<'_,'_>)->SelectionLayout {
 let v=unsafe {&mut *context};let ty=&v.a.as_ref().or(v.b.as_ref()).unwrap().ty;v.slots=v.builder.local_slots(ty).0;
 SelectionLayout {slots:v.slots.len(),components:ty.components.len()}
}
unsafe extern "C" fn selection_action(context:*mut SelectionValues<'_,'_>,action:SelectionAction,side:u32,index:usize) {
 let v=unsafe {&mut *context};
 match action {
  SelectionAction::Known=>{
   let(code,result)=if side==0{(&mut v.yes,&mut v.yes_result)}else{(&mut v.no,&mut v.no_result)};
   v.builder.code.extend(std::mem::take(code));v.output=result.take();
  },
  SelectionAction::Store=>{
   let(code,value)=if side==0{(&mut v.yes,v.a.as_ref().unwrap())}else{(&mut v.no,v.b.as_ref().unwrap())};
   code.push(Inst::new(InstKind::StoreLocal(v.slots[index],value.regs[index])));
  },
  SelectionAction::Emit=>v.builder.emit(InstKind::If {condition:v.condition.regs[0],yes:std::mem::take(&mut v.yes),no:std::mem::take(&mut v.no)}),
  SelectionAction::Load=>{
   let scalar=v.storage().ty.components[index].0;let reg=v.builder.reg(scalar);v.builder.emit(InstKind::LoadLocal(reg,v.slots[index]));v.regs.push(reg);
  },
  SelectionAction::BeginPaths=>v.selected=Some(bounds::Clause::impossible()),
  SelectionAction::Aliases=>{
   let value=if side==0{v.a.as_ref().unwrap()}else{v.b.as_ref().unwrap()};
   let aliases=bounds::Proof::capture(value).output_aliases(&v.regs,&v.builder.registers,&BTreeMap::new());
   v.selected=Some(bounds::Clause::either(v.selected.as_ref().unwrap(),&v.partition.assume(&aliases,side==0)));
  },
  SelectionAction::Path=>v.builder.path=bounds::Clause::both(&v.builder.path,v.selected.as_ref().unwrap()),
  SelectionAction::CommonProof=>v.proof=Some(bounds::Proof::common(v.a.as_ref().unwrap(),v.b.as_ref().unwrap(),Some(&v.partition),Some(&v.regs))),
  SelectionAction::CaptureProof=>v.proof=Some(bounds::Proof::capture(v.storage())),
  SelectionAction::Join=>{
   let mut a=v.yes_result.take().unwrap();a.join(v.no_result.take().unwrap(),false,Some(&v.partition));v.output=Some(a);
  },
  SelectionAction::Take=>v.output=if side==0{v.yes_result.take()}else{v.no_result.take()},
  SelectionAction::Finish=>{
   let result=v.output.as_mut().unwrap();v.builder.rebind_returned(result,std::mem::take(&mut v.regs));
   if let returns::Returned::Value(value)=result {value.proof=v.proof.take().unwrap();value.uniform=side!=0;value.literal=false;value.constant=None;}
  },
 }
}

#[repr(C)]
struct SelectionMergeInput {truth:u32,yes_live:u32,no_live:u32,returns:u32,uniform:u32}
#[repr(C)]
struct SelectionWriteFacts {present:u32,occupied:u32,stored:u32,value:u32}
#[repr(u32)]
enum SelectionMergeAction {
 RememberYes,RememberNo,Initialize,Split,BeginRegions,Region,InsertWrite,JoinWrite,ClearWrite,EndRegions,
 Environment,YesEffects,MergeEffects,YesPath,NoPath,EitherPath,ImpossiblePath,SelectExits,
 YesFlow,NoFlow,UnionFlow,RestoreInit,NoEnvironment,JoinEnvironment,Control,PrepareOwners,BeginOwners,HoldOwner,EndOwners,
}
#[repr(C)]
struct SelectionMergeHost<'a,'w> {
 context:*mut SelectionMerge<'a,'w>,next_region:unsafe extern "C" fn(*mut SelectionMerge<'a,'w>)->u32,
 next_write:unsafe extern "C" fn(*mut SelectionMerge<'a,'w>)->SelectionWriteFacts,
 owner_possible:unsafe extern "C" fn(*mut SelectionMerge<'a,'w>,u32)->u32,next_owner:unsafe extern "C" fn(*mut SelectionMerge<'a,'w>)->u32,
 action:unsafe extern "C" fn(*mut SelectionMerge<'a,'w>,SelectionMergeAction,u32),
}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::selection_merge"] fn selection_merge(input:SelectionMergeInput,host:SelectionMergeHost<'_,'_>);
}
type SelectionWrites=BTreeMap<(u32,u32),Option<Box<bounds::Proof>>>;
type SelectionRegion=(BTreeSet<(bool,u32,u32)>,SelectionWrites);
type SelectionRegions=BTreeMap<usize,SelectionRegion>;
type SelectionEffects=(BTreeSet<usize>,BTreeSet<usize>,BTreeSet<usize>);
struct SelectionMerge<'a,'w> {
 builder:&'a mut Builder<'w>,condition:&'a Value,prefix:usize,first_exit:usize,yes_exit:usize,yes_flow:Flow,no_flow:Flow,
 yes_env:Option<BTreeMap<String,Binding>>,no_env:Option<BTreeMap<String,Binding>>,yes_path:Option<bounds::Clause>,no_path:Option<bounds::Clause>,
 yes_init:Option<BTreeSet<Position>>,no_init:Option<BTreeSet<Position>>,selected_init:Option<BTreeSet<Position>>,yes_effects:Option<SelectionEffects>,
 outer_regions:Option<SelectionRegions>,regions:[Option<SelectionRegions>;2],no:Option<Vec<Inst>>,
 region_entries:Option<std::collections::btree_map::IntoIter<usize,SelectionRegion>>,region:Option<(usize,SelectionRegion)>,binding:usize,
 write_entries:Option<std::collections::btree_map::IntoIter<(u32,u32),Option<Box<bounds::Proof>>>>,write:Option<((u32,u32),Option<Box<bounds::Proof>>)>,
 owners:[Option<Vec<Binding>>;2],presences:[Option<owned_presence::Presence>;2],owner_entries:Option<std::vec::IntoIter<Binding>>,present:Option<owned_presence::Presence>,owner:Option<Binding>,
}
unsafe extern "C" fn merge_region(context:*mut SelectionMerge<'_,'_>)->u32 {
 let v=unsafe {&mut *context};v.region=v.region_entries.as_mut().unwrap().next();u32::from(v.region.is_some())
}
unsafe extern "C" fn merge_write(context:*mut SelectionMerge<'_,'_>)->SelectionWriteFacts {
 let v=unsafe {&mut *context};v.write=v.write_entries.as_mut().unwrap().next();
 let Some((range,value))=&v.write else{return SelectionWriteFacts {present:0,occupied:0,stored:0,value:0}};
 let stored=v.builder.memory_regions[&v.binding].1.get(range);
 SelectionWriteFacts {present:1,occupied:u32::from(stored.is_some()),stored:u32::from(stored.is_some_and(|v|v.is_some())),value:u32::from(value.is_some())}
}
unsafe extern "C" fn merge_owner_possible(context:*mut SelectionMerge<'_,'_>,side:u32)->u32 {
 let v=unsafe {&mut *context};u32::from(v.builder.owned_possible(v.presences[side as usize].as_ref().unwrap()))
}
unsafe extern "C" fn merge_owner(context:*mut SelectionMerge<'_,'_>)->u32 {
 let v=unsafe {&mut *context};v.owner=v.owner_entries.as_mut().unwrap().next();u32::from(v.owner.is_some())
}
unsafe extern "C" fn merge_action(context:*mut SelectionMerge<'_,'_>,action:SelectionMergeAction,side:u32) {
 let v=unsafe {&mut *context};
 match action {
  SelectionMergeAction::RememberYes=>v.selected_init=Some(v.yes_init.as_ref().unwrap().clone()),
  SelectionMergeAction::RememberNo=>v.selected_init=Some(v.no_init.as_ref().unwrap().clone()),
  SelectionMergeAction::Initialize=>v.builder.join_initialization(&owned_presence::Presence::input(v.condition),v.yes_init.take().unwrap(),v.no_init.take().unwrap()),
  SelectionMergeAction::Split=>{v.no=Some(v.builder.code.split_off(v.prefix));v.regions[1]=Some(std::mem::replace(&mut v.builder.memory_regions,v.outer_regions.take().unwrap()));},
  SelectionMergeAction::BeginRegions=>v.region_entries=Some(v.regions[side as usize].take().unwrap().into_iter()),
  SelectionMergeAction::Region=>{
   let(binding,(reads,writes))=v.region.take().unwrap();v.binding=binding;v.builder.memory_regions.entry(binding).or_default().0.extend(reads);v.write_entries=Some(writes.into_iter());
  },
  SelectionMergeAction::InsertWrite=>{let(range,value)=v.write.take().unwrap();v.builder.memory_regions.get_mut(&v.binding).unwrap().1.insert(range,value);},
  SelectionMergeAction::JoinWrite=>{
   let(range,value)=v.write.take().unwrap();v.builder.memory_regions.get_mut(&v.binding).unwrap().1.get_mut(&range).unwrap().as_deref_mut().unwrap().join(value.as_deref().unwrap(),1,None);
  },
  SelectionMergeAction::ClearWrite=>{let(range,_value)=v.write.take().unwrap();*v.builder.memory_regions.get_mut(&v.binding).unwrap().1.get_mut(&range).unwrap()=None;},
  SelectionMergeAction::EndRegions=>{v.regions[side as usize].take();v.region_entries=None;v.write_entries=None;},
  SelectionMergeAction::Environment=>v.no_env=Some(std::mem::replace(&mut v.builder.env,v.yes_env.take().unwrap())),
  SelectionMergeAction::YesEffects=>{(v.builder.reads,v.builder.writes,v.builder.cross_access)=v.yes_effects.take().unwrap();},
  SelectionMergeAction::MergeEffects=>{let(reads,writes,cross)=v.yes_effects.take().unwrap();v.builder.reads.extend(reads);v.builder.writes.extend(writes);v.builder.cross_access.extend(cross);},
  SelectionMergeAction::YesPath=>v.builder.path=v.yes_path.take().unwrap(),
  SelectionMergeAction::NoPath=>v.builder.path=v.no_path.take().unwrap(),
  SelectionMergeAction::EitherPath=>v.builder.path=bounds::Clause::either(v.yes_path.as_ref().unwrap(),v.no_path.as_ref().unwrap()),
  SelectionMergeAction::ImpossiblePath=>v.builder.path=bounds::Clause::impossible(),
  SelectionMergeAction::SelectExits=>v.builder.return_context.as_mut().unwrap().select_exits(v.first_exit,v.yes_exit,side!=0),
  SelectionMergeAction::YesFlow=>v.builder.flow=v.yes_flow,
  SelectionMergeAction::NoFlow=>v.builder.flow=v.no_flow,
  SelectionMergeAction::UnionFlow=>v.builder.flow=v.yes_flow.union(v.no_flow),
  SelectionMergeAction::RestoreInit=>v.builder.initialized=v.selected_init.take().unwrap(),
  SelectionMergeAction::NoEnvironment=>v.builder.env=v.no_env.take().unwrap(),
  SelectionMergeAction::JoinEnvironment=>{let when=owned_presence::Presence::input(v.condition);v.builder.join_environment_selected(v.no_env.as_mut().unwrap(),&when,&when.condition());},
  SelectionMergeAction::Control=>v.builder.control_uniform=side!=0,
  SelectionMergeAction::PrepareOwners=>v.presences=[Some(owned_presence::Presence::input(v.condition)),Some(owned_presence::Presence::input(v.condition).not())],
  SelectionMergeAction::BeginOwners=>{v.owner_entries=Some(v.owners[side as usize].take().unwrap().into_iter());v.present=v.presences[side as usize].take();},
  SelectionMergeAction::HoldOwner=>{
   let mut binding=v.owner.take().unwrap();let present=v.present.as_ref().unwrap();
   match &mut binding {Binding::OwnedValue(value)=>value.restrict_owned(present),Binding::ApplicationData(data)=>data.ownership=data.ownership.restricted(present),_=>{},}
   v.builder.hold_binding(binding);
  },
  SelectionMergeAction::EndOwners=>{v.owners[side as usize].take();v.presences[side as usize].take();v.owner_entries=None;v.present=None;},
 }
}

#[repr(C)]
#[derive(Clone,Copy)]
struct SelectionCursor {phase:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct SelectionFacts {continuing:u32,present:u32,other:u32,outer:u32,condition:u32}
#[repr(u32)]
enum SelectionNeed {Prepare,Modified,EnterYes,Yes,Missing,EnterNo,No,Same,Finish}
#[repr(C)]
struct SelectionStep {cursor:SelectionCursor,need:SelectionNeed,uniform:u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::selection_step"] fn selection_step(cursor:SelectionCursor,facts:SelectionFacts)->SelectionStep;
}
struct SelectionEntry {
 prefix:usize,outer_regions:SelectionRegions,environment:BTreeMap<String,Binding>,original:BTreeSet<String>,
 path:bounds::Clause,initialized:BTreeSet<Position>,effects:SelectionEffects,
}
struct SelectionFirst {
 prefix:usize,outer_regions:SelectionRegions,original:BTreeSet<String>,yes_flow:Flow,yes_env:BTreeMap<String,Binding>,yes_path:bounds::Clause,
 yes_init:BTreeSet<Position>,yes_effects:SelectionEffects,yes:Vec<Inst>,yes_regions:SelectionRegions,first_exit:usize,yes_exit:usize,
 a_result:Option<returns::Returned>,yes_owners:Vec<Binding>,
}

#[repr(C)]
#[derive(Clone,Copy)]
struct OwnerName {data:*const u8,length:usize}
#[repr(C)]
#[derive(Clone,Copy)]
struct OwnerCursor {phase:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct OwnerFacts {name:OwnerName,present:u32,kind:u32,answer:u32}
#[repr(u32)]
enum OwnerNeed {NextBinding,Original,Kind,MayOwn,Reference,Borrow,NextResource,UnionResource,Restrict,BorrowPending,ReferencesPending,StartSources,NextSource,Overlap,Keep,Complete}
#[repr(C)]
struct OwnerStep {cursor:OwnerCursor,need:OwnerNeed}
#[repr(C)]
struct OwnerNamesHost<'a,'w> {context:*mut OwnerMoves<'a,'w>,name:unsafe extern "C" fn(*const OwnerMoves<'a,'w>,usize)->OwnerName,
 exchange:unsafe extern "C" fn(*mut OwnerMoves<'a,'w>,usize,usize),begin:unsafe extern "C" fn(*mut OwnerMoves<'a,'w>),take:unsafe extern "C" fn(*mut OwnerMoves<'a,'w>)}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::owner_step"] fn owner_step(cursor:OwnerCursor,facts:OwnerFacts)->OwnerStep;
 #[link_name="bootstrap.function_body::move_owners"] fn move_owners(count:usize,host:OwnerNamesHost<'_,'_>);
}
struct OwnerMoves<'a,'w> {builder:&'a mut Builder<'w>,names:Option<Vec<String>>,ordered:Option<std::vec::IntoIter<String>>,owners:Vec<Binding>}
unsafe extern "C" fn owner_name(context:*const OwnerMoves<'_,'_>,index:usize)->OwnerName {
 let name=&unsafe {&*context}.names.as_ref().unwrap()[index];OwnerName {data:name.as_ptr(),length:name.len()}
}
unsafe extern "C" fn exchange_owner_names(context:*mut OwnerMoves<'_,'_>,a:usize,b:usize) {unsafe {&mut *context}.names.as_mut().unwrap().swap(a,b);}
unsafe extern "C" fn begin_owner_moves(context:*mut OwnerMoves<'_,'_>) {let values=unsafe {&mut *context};values.ordered=Some(values.names.take().unwrap().into_iter());}
unsafe extern "C" fn take_owner_move(context:*mut OwnerMoves<'_,'_>) {
 let values=unsafe {&mut *context};let name=values.ordered.as_mut().unwrap().next().unwrap();values.owners.push(values.builder.env.remove(&name).unwrap());
}
