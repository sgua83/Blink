//! 顺序块合成真实退出边；循环结束只合流条件失败与 break 的状态。
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Flow(u8);
impl Flow {
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
pub(super) struct LoopState {
    env: BTreeMap<String, Binding>,
    initialized: BTreeSet<Position>,
}
pub(super) struct LoopContext {
    names: BTreeSet<String>,
    resources: BTreeMap<String, (Option<Target>, application::Ownership)>,
    exits: Vec<LoopState>,
}
impl Builder<'_> {
    pub(super) fn check_application_loop(&self, span: Span) -> Result<()> {
        let entry = &self.loop_contexts.last().unwrap().resources;
        for (name, binding) in &self.env {
            if let Binding::ApplicationData(data) = binding
                && let Some((residency, ownership)) = entry.get(name)
            {
                if *ownership != data.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的 dataset 所有权，不能在回边重复消费",
                    );
                }
                if *residency != data.residency {
                    return self.fail(
                        span,
                        "B266",
                        "循环继续前须归还入口相同的 CPU／GPU 存储位置；在循环内完成配对的显式传输",
                    );
                }
            }
        }
        Ok(())
    }
    pub(super) fn release_application_loop(&mut self) {
        let names = self.loop_contexts.last().unwrap().names.clone();
        self.release_application_data(Some(&names));
    }
    pub(super) fn join_environment(
        &mut self,
        other: &BTreeMap<String, Binding>,
        branch: Option<&Value>,
    ) {
        for (name, binding) in &mut self.env {
            match (binding, &other[name]) {
                (Binding::Value(a), Binding::Value(b)) => {
                    a.proof.join(&b.proof, a.regs.len(), branch);
                    a.uniform &= b.uniform;
                }
                (Binding::Owned(a), Binding::Owned(b)) => {
                    a.proof
                        .join(&b.proof, a.place.ty().components.len(), branch);
                    a.uniform &= b.uniform;
                }
                (Binding::Data(a), Binding::Data(b)) => a.length.join(&b.length, 1, None),
                (Binding::ApplicationData(a), Binding::ApplicationData(b)) => {
                    if a.ownership != b.ownership {
                        a.ownership = application::Ownership::Moved;
                    }
                    a.length.proof.join(&b.length.proof, 1, None);
                    if a.residency != b.residency {
                        a.residency = None;
                    }
                }
                (Binding::Tile(a), Binding::Tile(b)) => a.length.join(&b.length, 1, None),
                _ => {}
            }
        }
    }
    pub(super) fn enter_loop(&mut self, environment: &BTreeMap<String, Binding>) {
        self.loop_contexts.push(LoopContext {
            names: environment.keys().cloned().collect(),
            resources: environment
                .iter()
                .filter_map(|(name, binding)| match binding {
                    Binding::ApplicationData(data) => {
                        Some((name.clone(), (data.residency, data.ownership)))
                    }
                    _ => None,
                })
                .collect(),
            exits: Vec::new(),
        });
    }
    pub(super) fn loop_state(&self) -> LoopState {
        let names = &self.loop_contexts.last().unwrap().names;
        LoopState {
            env: self
                .env
                .iter()
                .filter(|(n, _)| names.contains(*n))
                .map(|(n, b)| (n.clone(), b.clone()))
                .collect(),
            initialized: self.initialized.clone(),
        }
    }
    pub(super) fn capture_break(&mut self) {
        let state = self.loop_state();
        self.loop_contexts.last_mut().unwrap().exits.push(state);
    }
    pub(super) fn leave_loop(
        &mut self,
        natural: Option<LoopState>,
        condition: &mut Vec<Inst>,
        test: Reg,
        body: &mut Vec<Inst>,
    ) -> bool {
        let mut context = self.loop_contexts.pop().unwrap();
        let break_states: Vec<_> = context.exits.iter().map(|exit| exit.env.clone()).collect();
        let natural_env = natural.as_ref().map(|exit| exit.env.clone());
        context.exits.extend(natural);
        let mut exits = context.exits.into_iter();
        let Some(first) = exits.next() else {
            self.env.retain(|name, _| context.names.contains(name));
            return false;
        };
        self.env = first.env;
        self.initialized = first.initialized;
        for exit in exits {
            self.initialized = self
                .initialized
                .intersection(&exit.initialized)
                .copied()
                .collect();
            self.join_environment(&exit.env, None);
        }
        let releases: Vec<_> = break_states
            .iter()
            .map(|environment| self.application_join_releases(environment, &self.env))
            .collect();
        insert_break_releases(body, &mut releases.into_iter());
        if let Some(environment) = natural_env {
            let no = self.application_join_releases(&environment, &self.env);
            if !no.is_empty() {
                condition.push(Inst::If {
                    condition: test,
                    yes: Vec::new(),
                    no,
                });
            }
        }
        true
    }
}
// 每个 break 仍属于原循环；内层循环已沿自己的出口完成交接。
fn insert_break_releases(code: &mut Vec<Inst>, releases: &mut impl Iterator<Item = Vec<Inst>>) {
    let mut result = Vec::new();
    for mut instruction in std::mem::take(code) {
        match &mut instruction {
            Inst::Break => result.extend(releases.next().unwrap()),
            Inst::If { yes, no, .. } => {
                insert_break_releases(yes, releases);
                insert_break_releases(no, releases);
            }
            Inst::Scope(body) => insert_break_releases(body, releases),
            _ => {}
        }
        result.push(instruction);
    }
    *code = result;
}
