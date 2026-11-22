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
        let (mut a_result,yes_owners)=unsafe { self.world.call(self.selection_arm(yes.0,&original,yes.1)).await }?;
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
        let mut yes = self.code.split_off(prefix);
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
        let mut no = self.code.split_off(prefix);
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
        if let Some(bits) = &c.constant {
            if bits[0] != 0 {
                self.code.extend(yes);
                return Ok(a_result);
            }
            self.code.extend(no);
            return Ok(b_result);
        }
        let a = a_result.as_ref().map(|v| self.returned_storage(v));
        let b = b_result.as_ref().map(|v| self.returned_storage(v));
        let Some(storage) = a.as_ref().or(b.as_ref()) else {
            self.emit(InstKind::If {
                condition: c.regs[0],
                yes,
                no,
            });
            return Ok(None);
        };
        let ty = storage.ty.clone();
        let slots = self.local_slots(&ty).0;
        if let Some(a) = &a {
            for (slot, reg) in slots.iter().zip(&a.regs) {
                yes.push(Inst::new(InstKind::StoreLocal(*slot, *reg)));
            }
        }
        if let Some(b) = &b {
            for (slot, reg) in slots.iter().zip(&b.regs) {
                no.push(Inst::new(InstKind::StoreLocal(*slot, *reg)));
            }
        }
        self.emit(InstKind::If {
            condition: c.regs[0],
            yes,
            no,
        });
        let regs:Vec<Reg> = slots
            .iter()
            .zip(&ty.components)
            .map(|(slot, (scalar, _))| {
                let reg = self.reg(*scalar);
                self.emit(InstKind::LoadLocal(reg, *slot));
                reg
            })
            .collect();
        let mut selected = bounds::Clause::impossible();
        for (value, truth) in [(a.as_ref(), true), (b.as_ref(), false)] {
            let Some(value) = value else { continue; };
            let aliases = bounds::Proof::capture(value).output_aliases(&regs, &self.registers, &BTreeMap::new());
            selected = bounds::Clause::either(&selected, &partition.assume(&aliases, truth));
        }
        self.path = bounds::Clause::both(&self.path, &selected);
        let proof = match (&a, &b) {
            (Some(a), Some(b)) => bounds::Proof::common(a,b,Some(&partition),Some(&regs)),
            _ => bounds::Proof::capture(storage),
        };
        let mut result = match (a_result.take(), b_result) {
            (Some(mut a), Some(b)) => {
                a.join(b,false,Some(&partition));
                a
            }
            (Some(a), None) | (None, Some(a)) => a,
            _ => unreachable!(),
        };
        self.rebind_returned(&mut result, regs);
        if let returns::Returned::Value(value) = &mut result {
            value.proof = proof;
            value.uniform = c.uniform && a.iter().chain(b.iter()).all(|v| v.uniform);
            value.literal = false;
            value.constant = None;
        }
        Ok(Some(result))
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
