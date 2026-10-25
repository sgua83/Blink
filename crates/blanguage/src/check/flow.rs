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
    exits: Vec<LoopState>,
}
impl Builder<'_> {
    pub(super) fn enter_loop(&mut self, environment: &BTreeMap<String, Binding>) {
        self.loop_contexts.push(LoopContext {
            names: environment.keys().cloned().collect(),
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
    pub(super) fn leave_loop(&mut self, natural: Option<LoopState>) -> bool {
        let mut context = self.loop_contexts.pop().unwrap();
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
            for (name, binding) in &mut self.env {
                match (binding, &exit.env[name]) {
                    (Binding::Value(a), Binding::Value(b)) => a.proof.join(&b.proof, a.regs.len()),
                    (Binding::Owned(a), Binding::Owned(b)) => {
                        a.proof.join(&b.proof, a.place.ty().components.len());
                        a.uniform &= b.uniform;
                    }
                    (Binding::Data(a), Binding::Data(b)) => a.length.join(&b.length, 1),
                    (Binding::Tile(a), Binding::Tile(b)) => a.length.join(&b.length, 1),
                    _ => {}
                }
            }
        }
        true
    }
}
