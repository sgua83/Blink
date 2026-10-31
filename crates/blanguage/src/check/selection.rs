//! 选择式先检查两侧，在运行时只求值一侧；结果仍是普通值。
use super::*;
impl Builder<'_> {
    #[inline(never)]
    pub(super) fn selection(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
    ) -> Result<Option<Value>> {
        self.selection_expected(condition, yes, no, span, None)
    }
    pub(super) fn selection_expected(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
        expected: Option<&Ty>,
    ) -> Result<Option<Value>> {
        let result = self.selection_returned(condition, yes, no, span, expected)?;
        if !self.flow.continues() {
            return Ok(None);
        }
        let value = self.ordinary_return(result, span)?;
        self.value_result(value, span).map(Some)
    }
    #[inline(never)]
    pub(super) fn selection_returned(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
        expected: Option<&Ty>,
    ) -> Result<Option<returns::Returned>> {
        let Some(c) = self.expr(condition)? else {
            return Ok(None);
        };
        self.require_bool(&c, condition.span)?;
        self.remember_loop_control(&c);
        let partition = Arc::new(bounds::Condition::of(&c));
        let outer = std::mem::take(&mut self.code);
        let outer_regions = std::mem::take(&mut self.memory_regions);
        let environment = self.env.clone();
        let path = self.path.clone();
        let initialized = self.application_initialization_state();
        let effects = (
            self.reads.clone(),
            self.writes.clone(),
            self.cross_access.clone(),
        );
        let mut yes_modified = BTreeSet::new();
        expression_assignments(self.world, &self.definition, yes, &mut yes_modified);
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.refine_bounds(&partition, true);
        self.flow = Flow::NEXT;
        let first_exit = self
            .return_context
            .as_ref()
            .map_or(0, |context| context.exit_count());
        let mut a_result = self.evaluate_returned(yes, expected)?;
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
        let mut yes = std::mem::take(&mut self.code);
        let yes_regions = std::mem::take(&mut self.memory_regions);
        self.restore_expression_environment(environment, &yes_modified);
        self.path = path;
        self.refine_bounds(&partition, false);
        self.flow = Flow::NEXT;
        let b_result = self.evaluate_returned(no, expected)?;
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
        self.join_initialization(&c, yes_init, no_init);
        let mut no = std::mem::take(&mut self.code);
        let no_regions = std::mem::replace(&mut self.memory_regions, outer_regions);
        let selected = c.constant.as_ref().map(|bits| bits[0] != 0);
        for (retain, regions) in [
            (selected != Some(false), yes_regions),
            (selected != Some(true), no_regions),
        ] {
            if retain {
                for (binding, (reads, writes)) in regions {
                    let entry = self.memory_regions.entry(binding).or_default();
                    entry.0.extend(reads); entry.1.extend(writes);
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
                self.join_environment_selected(&mut no_env, &owned_presence::Presence::input(&c));
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
        self.code = outer;
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
            self.code.push(Inst::If {
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
                yes.push(Inst::StoreLocal(*slot, *reg));
            }
        }
        if let Some(b) = &b {
            for (slot, reg) in slots.iter().zip(&b.regs) {
                no.push(Inst::StoreLocal(*slot, *reg));
            }
        }
        self.code.push(Inst::If {
            condition: c.regs[0],
            yes,
            no,
        });
        let regs = slots
            .iter()
            .zip(&ty.components)
            .map(|(slot, (scalar, _))| {
                let reg = self.reg(*scalar);
                self.code.push(Inst::LoadLocal(reg, *slot));
                reg
            })
            .collect();
        let proof = match (&a, &b) {
            (Some(a), Some(b)) => bounds::Proof::selection(&c, a, b),
            _ => bounds::Proof::capture(storage),
        };
        let mut result = match (a_result.take(), b_result) {
            (Some(mut a), Some(b)) => {
                a.join(b);
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
