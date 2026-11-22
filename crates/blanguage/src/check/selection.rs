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
        if !self.flow.continues() {
            return Ok(None);
        }
        let value = self.ordinary_return(result, span)?;
        self.value_result(value, span).map(Some)
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
        self.remember_loop_control(&c);
        let partition = Arc::new(bounds::Condition::of(&c));
        let prefix = self.code.len();
        let outer_regions = std::mem::take(&mut self.memory_regions);
        let environment = self.env.clone();
        let original=environment.keys().cloned().collect::<BTreeSet<_>>();
        let path = self.path.clone();
        let initialized = self.application_initialization_state();
        let effects = (
            self.reads.clone(),
            self.writes.clone(),
            self.cross_access.clone(),
        );
        let mut yes_modified = BTreeSet::new();
        modified(self, &mut yes_modified).await?;
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.refine_bounds(&partition, true);
        self.flow = Flow::NEXT;
        let first_exit = self
            .return_context
            .as_ref()
            .map_or(0, |context| context.exit_count());
        let (a_result,yes_owners)=unsafe { self.world.call(self.selection_arm(yes.0,&original,yes.1)).await }?;
        let yes_exit = self
            .return_context
            .as_ref()
            .map_or(0, |context| context.exit_count());
        if self.flow.continues() && a_result.is_none() {
            return self.fail(span, "B228", "选择式需要值");
        }
        let yes_flow = self.flow;
        let yes_env = self.env.clone();
        let yes_path = self.path.clone();
        let yes_effects = (
            std::mem::replace(&mut self.reads, effects.0),
            std::mem::replace(&mut self.writes, effects.1),
            std::mem::replace(&mut self.cross_access, effects.2),
        );
        let yes_init = std::mem::replace(&mut self.initialized, initialized);
        let yes_init = self.application_empty_initialization(yes_init);
        let yes = self.code.split_off(prefix);
        let yes_regions = std::mem::take(&mut self.memory_regions);
        self.restore_expression_environment(environment, &yes_modified);
        self.path = path;
        self.refine_bounds(&partition, false);
        self.flow = Flow::NEXT;
        let (b_result,no_owners)=unsafe { self.world.call(self.selection_arm(no.0,&original,no.1)).await }?;
        if self.flow.continues() && b_result.is_none() {
            return self.fail(span, "B228", "选择式需要值");
        }
        if let (Some(a), Some(b)) = (&a_result, &b_result) {
            let a = self.returned_storage(a);
            let b = self.returned_storage(b);
            self.require_same(&a.ty, &b.ty, span)?;
        }
        let no_flow = self.flow;
        let no_path = self.path.clone();
        let no_init = self.take_application_initialization_state();
        let yes_initial = yes_init.clone();
        let no_initial = no_init.clone();
        self.join_initialization(&owned_presence::Presence::input(&c),yes_init,no_init);
        let no = self.code.split_off(prefix);
        let no_regions = std::mem::replace(&mut self.memory_regions, outer_regions);
        let selected = c.constant.as_ref().map(|bits| bits[0] != 0);
        for (retain, regions) in [
            (selected != Some(false), yes_regions),
            (selected != Some(true), no_regions),
        ] {
            if retain {
                for (binding, (reads, writes)) in regions {
                    let entry = self.memory_regions.entry(binding).or_default();
                    entry.0.extend(reads);
                    for (range, value) in writes {
                        entry.1.entry(range).and_modify(|stored| match (stored.as_deref_mut(), value.as_deref()) {
                            (Some(stored), Some(value)) => stored.join(value, 1, None),
                            _ => *stored = None,
                        }).or_insert(value);
                    }
                }
            }
        }
        let mut no_env = std::mem::replace(&mut self.env, yes_env);
        if let Some(bits) = &c.constant {
            if bits[0] != 0 {
                (self.reads, self.writes, self.cross_access) = yes_effects;
            } else {
                self.env = no_env.clone();
            }
        } else {
            self.reads.extend(yes_effects.0);
            self.writes.extend(yes_effects.1);
            self.cross_access.extend(yes_effects.2);
        }
        if let Some(bits) = &c.constant {
            self.path = if bits[0] != 0 {
                yes_path.clone()
            } else {
                no_path.clone()
            };
            if let Some(context) = &mut self.return_context {
                context.select_exits(first_exit, yes_exit, bits[0] != 0);
            }
            self.flow = if bits[0] != 0 { yes_flow } else { no_flow };
            self.initialized = if bits[0] != 0 {
                yes_initial
            } else {
                no_initial
            };
        } else {
            self.path = match (yes_flow.continues(), no_flow.continues()) {
                (true, true) => bounds::Clause::either(&yes_path, &no_path),
                (true, false) => yes_path,
                (false, true) => no_path,
                (false, false) => bounds::Clause::impossible(),
            };
            if yes_flow.continues() && no_flow.continues() {
                let when = owned_presence::Presence::input(&c);
                self.join_environment_selected(&mut no_env, &when, &when.condition());
            }
            if yes_flow.continues() && !no_flow.continues() {
                self.initialized = yes_initial;
            } else if !yes_flow.continues() && no_flow.continues() {
                self.env = no_env;
                self.initialized = no_initial;
            }
            self.flow = yes_flow.union(no_flow);
        }
        self.control_uniform = control;
        for (owners,present) in [(yes_owners,owned_presence::Presence::input(&c)),(no_owners,owned_presence::Presence::input(&c).not())] {
            if !self.owned_possible(&present){continue;}
            for mut binding in owners {
                match &mut binding {
                    Binding::OwnedValue(value)=>value.restrict_owned(&present),
                    Binding::ApplicationData(data)=>data.ownership=data.ownership.restricted(&present),
                    _=>{},
                }
                self.hold_binding(binding);
            }
        }
        let input=SelectionResultInput {truth:c.constant.as_ref().map_or(0,|bits|if bits[0]!=0{2}else{1}),uniform:u32::from(c.uniform)};
        let mut values=SelectionValues {builder:self,condition:c,partition,yes,no,yes_result:a_result,no_result:b_result,
            a:None,b:None,slots:Vec::new(),regs:Vec::new(),selected:None,proof:None,output:None};
        unsafe {selection_result(input,SelectionHost {context:&mut values,storage:selection_storage,layout:selection_layout,action:selection_action})};
        Ok(values.output)
    }
}

impl Builder<'_> {
    async fn selection_arm(&mut self,span:Span,original:&BTreeSet<String>,lower:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<returns::Returned>>)->Result<(Option<returns::Returned>,Vec<Binding>)> {
        unsafe { self.world.call(self.temporary_scope(span,async |builder|{
            let value=lower(builder).await?;
            let retained=value.as_ref().and_then(returns::Returned::view_value).map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();
            let borrow=retained.owners().restricted(&builder.path);
            let mut names=Vec::new();
            for(name,binding)in builder.env.iter().filter(|(name,_)|name.starts_with("\0argument:")&&!original.contains(*name))
                .filter(|(_,binding)|matches!(binding,Binding::OwnedValue(value)if value.ownership.may_own()&&!matches!(value.place(),Some(Place::Reference{..})))||matches!(binding,Binding::ApplicationData(data)if data.ownership.may_own()))
                {
                    let mut references=unsafe { builder.world.call(binding.view_borrow(builder)).await }?.owners();
                    for data in binding.application_resources(){references.union(&builder.owned_resource_borrow(data).owners());}
                    references=references.restricted(&builder.path);
                    if borrow.pending()||references.pending()||borrow.sources.entries().any(|(source,when)|references.overlaps_source(source,&when)){names.push(name.clone());}
                }
            names.sort_by_key(|name|name.strip_prefix("\0argument:").unwrap().parse::<usize>().unwrap());
            let owners=names.into_iter().map(|name|builder.env.remove(&name).unwrap()).collect();
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
