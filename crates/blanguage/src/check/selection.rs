//! 选择式先检查两侧，在运行时只求值一侧；结果仍是普通值。
use super::*;
impl Builder<'_> {
    pub(super) fn selection(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        span: Span,
    ) -> Result<Value> {
        let c = self.expr(condition)?;
        self.require_bool(&c, condition.span)?;
        let outer = std::mem::take(&mut self.code);
        let environment = self.env.clone();
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
        self.refine_bounds(&c, true);
        let a = self.expr(yes)?;
        let yes_env = self.env.clone();
        let yes_effects = (
            std::mem::replace(&mut self.reads, effects.0),
            std::mem::replace(&mut self.writes, effects.1),
            std::mem::replace(&mut self.cross_access, effects.2),
        );
        let yes_init = std::mem::replace(&mut self.initialized, initialized);
        let yes_init = self.application_empty_initialization(yes_init);
        let mut yes = std::mem::take(&mut self.code);
        self.restore_expression_environment(environment, &yes_modified);
        self.refine_bounds(&c, false);
        let b = self.expr(no)?;
        let no_init = self.take_application_initialization_state();
        self.join_initialization(&c, yes_init, no_init);
        let mut no = std::mem::take(&mut self.code);
        let no_env = std::mem::replace(&mut self.env, yes_env);
        if let Some(bits) = &c.constant {
            if bits[0] != 0 {
                (self.reads, self.writes, self.cross_access) = yes_effects;
            } else {
                self.env = no_env;
            }
        } else {
            let yes_environment = self.env.clone();
            yes.extend(self.application_join_releases(&yes_environment, &no_env));
            no.extend(self.application_join_releases(&no_env, &yes_environment));
            self.join_environment(&no_env, Some(&c));
            self.reads.extend(yes_effects.0);
            self.writes.extend(yes_effects.1);
            self.cross_access.extend(yes_effects.2);
        }
        self.control_uniform = control;
        self.code = outer;
        self.require_same(&a.ty, &b.ty, span)?;
        if let Some(bits) = &c.constant {
            let literal = a.literal && b.literal;
            let mut value = if bits[0] != 0 {
                self.code.extend(yes);
                a
            } else {
                self.code.extend(no);
                b
            };
            value.literal = literal;
            return Ok(value);
        }
        let slots: Vec<_> =
            a.ty.components
                .iter()
                .map(|(s, _)| {
                    let slot = self.locals.len();
                    self.locals.push(*s);
                    slot
                })
                .collect();
        for ((slot, a), b) in slots.iter().zip(&a.regs).zip(&b.regs) {
            yes.push(Inst::StoreLocal(*slot, *a));
            no.push(Inst::StoreLocal(*slot, *b));
        }
        self.code.push(Inst::If {
            condition: c.regs[0],
            yes,
            no,
        });
        let regs = slots
            .into_iter()
            .zip(&a.ty.components)
            .map(|(slot, (s, _))| {
                let dst = self.reg(*s);
                self.code.push(Inst::LoadLocal(dst, slot));
                dst
            })
            .collect();
        let proof = if a.ty == Ty::scalar(Scalar::Bool) {
            bounds::Proof::condition(bounds::Condition::select(&c, &a, &b))
        } else {
            bounds::Proof::common(&a, &b)
        };
        Ok(Value {
            proof,
            literal: false,
            ty: a.ty,
            regs,
            uniform: c.uniform && a.uniform && b.uniform,
            constant: None,
        })
    }
}
