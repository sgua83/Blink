//! 顺序块合成真实退出边；循环结束只合流条件失败与 break 的状态。
use super::*;
impl Place {
    fn join_position_range(&mut self, other: &Self) {
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
#[derive(Clone)]
pub(super) struct LoopState {
    env: BTreeMap<String, Binding>,
    path: bounds::Clause,
    initialized: BTreeSet<Position>,
    control: bool,
    cursor: Option<Value>,
}
pub(super) struct LoopContext {
    names: BTreeSet<String>,
    resources: BTreeMap<String, Binding>,
    exits: Vec<LoopState>,
    initial: bool,
    arrays: bool,
    continues: Vec<LoopState>,
    iterator: Option<String>,
    initial_cursor: Option<Value>,
}
impl Builder<'_> {
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
                        initialized_application_word(&state.initialized, scope, index, *word)
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
        body: &[Stmt],
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
                self.position_initialized_in(&position, &state.initialized, &state.env)
            }) {
                continue;
            }
            if begin.maximum() < limit.minimum() && !position.uses_index(index) {
                exit.initialized.insert(position.clone());
            }
            if begin.minimum() == begin.maximum()
                && limit.minimum() == limit.maximum()
                && let Position::Indexed(choices, at) = &position
                && *at == index
                && let Some(covered) =
                    choices.get(begin.minimum() as usize..limit.minimum() as usize)
            {
                exit.initialized.extend(covered.iter().cloned());
            }
        }
    }
    pub(super) fn application_initial_loop(&self, body: &[Stmt]) -> bool {
        self.env.iter().any(|(name, binding)| {
            application::rebound(body, name)
                && match binding {
                    Binding::ApplicationData(data) => {
                        data.ownership == application::Ownership::Moved
                    }
                    Binding::OwnedValue(result) => {
                        result.ownership == application::Ownership::Moved || result.incomplete()
                    }
                    _ => false,
                }
        })
    }
    pub(super) fn configure_initial_loop(
        &mut self,
        initial: bool,
        iterator: Option<String>,
        cursor: Option<Value>,
    ) {
        let arrays = iterator.is_some() && self.incomplete_arrays();
        let context = self.loop_contexts.last_mut().unwrap();
        context.arrays = arrays;
        context.initial = initial;
        context.iterator = iterator;
        context.initial_cursor = cursor;
    }
    pub(super) fn initial_loop_cursor(&self) -> Option<Value> {
        self.loop_contexts
            .last()
            .and_then(|context| context.initial_cursor.clone())
    }
    pub(super) fn capture_loop_cursor(&mut self, value: Value) {
        if let Some(name) = self
            .loop_contexts
            .last()
            .and_then(|context| context.iterator.clone())
        {
            self.env.insert(name, Binding::Value(value));
        }
    }
    pub(super) fn check_application_loop(&mut self, span: Span) -> Result<()> {
        if self.loop_contexts.last().unwrap().initial {
            let state = self.loop_state();
            self.loop_contexts.last_mut().unwrap().continues.push(state);
            return Ok(());
        }
        let entry = &self.loop_contexts.last().unwrap().resources;
        for (name, binding) in &self.env {
            if let Binding::OwnedValue(result) = binding
                && let Some(Binding::OwnedValue(before)) = entry.get(name)
            {
                if result.ownership != before.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的结果所有权，不能在回边重复消费",
                    );
                }
                if result.ownership == application::Ownership::Owned
                    && !before.loop_compatible(
                        result,
                        &mut self.residency,
                        &self.definition.source,
                        span,
                    )
                {
                    return self.fail(span,"B266","循环回边尚未能合流结果载荷的分支与CPU／GPU存储位置；需要相同的资源生命周期");
                }
            }
            if let Binding::ApplicationData(data) = binding
                && let Some(Binding::ApplicationData(before)) = entry.get(name)
            {
                if before.ownership != data.ownership {
                    return self.fail(
                        span,
                        "B227",
                        "循环继续前须保有入口的 dataset 所有权，不能在回边重复消费",
                    );
                }
                if before.ownership != application::Ownership::Moved
                    && !self.residency.same(
                        &before.residency,
                        &data.residency,
                        &self.definition.source,
                        span,
                    )
                {
                    return self.fail(
                        span,
                        "B266",
                        "循环继续前须归还入口相同的 CPU／GPU 存储位置；在循环内完成配对的显式传输",
                    );
                }
            }
        }
        if self.loop_contexts.last().unwrap().arrays || self.env.values().any(|binding| binding.application_resources().iter().any(|data|data.initialization.is_some())) {
            let state = self.loop_state();
            self.loop_contexts.last_mut().unwrap().continues.push(state);
        }
        Ok(())
    }
    pub(super) fn release_application_loop(&mut self) -> Result<()> {
        let names = self.loop_contexts.last().unwrap().names.clone();
        self.release_application_data(Some(&names))
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
                    a.place.join_position_range(&b.place);
                    a.proof
                        .join(&b.proof, a.place.ty().components.len(), branch);
                    a.uniform &= b.uniform;
                }
                (Binding::Data(a), Binding::Data(b)) => a.length.join(&b.length, 1, None),
                (Binding::OwnedValue(a), Binding::OwnedValue(b)) => {
                    if a.ownership != b.ownership {
                        a.ownership = application::Ownership::Moved;
                    }
                    a.join(b.clone());
                }
                (Binding::ApplicationData(a), Binding::ApplicationData(b)) => {
                    if a.ownership != b.ownership {
                        a.ownership = application::Ownership::Moved;
                    }
                    a.handle.proof.join(&b.handle.proof, 1, None);
                    a.length.proof.join(&b.length.proof, 1, None);
                    a.length.uniform &= b.length.uniform;
                    if a.length.constant != b.length.constant {
                        a.length.constant = None;
                    }
                    a.residency.join(&b.residency);
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
                    Binding::ApplicationData(_) | Binding::OwnedValue(_) => {
                        Some((name.clone(), binding.clone()))
                    }
                    _ => None,
                })
                .collect(),
            exits: Vec::new(),
            initial: false,
            arrays: false,
            continues: Vec::new(),
            iterator: None,
            initial_cursor: None,
        });
    }
    pub(super) fn loop_state(&self) -> LoopState {
        let names = &self.loop_contexts.last().unwrap().names;
        LoopState {
            path: self.path.clone(),
            env: self
                .env
                .iter()
                .filter(|(n, _)| names.contains(*n))
                .map(|(n, b)| (n.clone(), b.clone()))
                .collect(),
            initialized: self.application_initialization_state(),
            control: self.control_uniform,
            cursor: self
                .loop_contexts
                .last()
                .unwrap()
                .iterator
                .as_ref()
                .and_then(|name| match self.env.get(name) {
                    Some(Binding::Value(value)) => Some(value.clone()),
                    _ => None,
                }),
        }
    }
    fn restore_initial_edge(&mut self, state: LoopState) {
        self.env = state.env;
        self.path = state.path;
        self.initialized = state.initialized;
        self.control_uniform = state.control;
    }
    pub(super) fn expand_initial_loop<F>(
        &mut self,
        code: Vec<Inst>,
        next: bool,
        mut continuation: F,
    ) -> Result<(Vec<Inst>, Flow)>
    where
        F: FnMut(&mut Self, Option<Value>) -> Result<()>,
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
        let mut code = expand_initial_code(
            self,
            code,
            &mut breaks.into_iter(),
            &mut continues.into_iter(),
            &mut exits,
            &mut flow,
            &mut continuation,
        )?;
        if let Some(state) = natural {
            code.extend(compile_initial_continuation(
                self,
                state,
                &mut exits,
                &mut flow,
                &mut continuation,
            )?);
        }
        self.loop_contexts.last_mut().unwrap().exits = exits;
        Ok((code, flow))
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
            self.path = bounds::Clause::impossible();
            return false;
        };
        self.env = first.env;
        self.path = first.path;
        self.initialized = first.initialized;
        for exit in exits {
            self.path = bounds::Clause::either(&self.path, &exit.path);
            self.initialized =
                ownership::intersect_initialized(&self.initialized, &exit.initialized);
            self.join_environment(&exit.env, None);
        }
        let final_environment = self.env.clone();
        let releases: Vec<_> = break_states
            .iter()
            .map(|environment| self.application_join_releases(environment, &final_environment))
            .collect();
        insert_break_releases(body, &mut releases.into_iter());
        if let Some(environment) = natural_env {
            let no = self.application_join_releases(&environment, &final_environment);
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

// Continue is the real next-iteration edge. Compile its continuation directly, without an ownership flag.
fn compile_initial_continuation<'a, F>(
    builder: &mut Builder<'a>,
    state: LoopState,
    exits: &mut Vec<LoopState>,
    flow: &mut Flow,
    continuation: &mut F,
) -> Result<Vec<Inst>>
where
    F: FnMut(&mut Builder<'a>, Option<Value>) -> Result<()>,
{
    let cursor = state.cursor.clone();
    builder.restore_initial_edge(state);
    builder.flow = Flow::NEXT;
    continuation(builder, cursor)?;
    *flow = flow.union(builder.flow);
    let mut code = std::mem::take(&mut builder.code);
    if builder.flow.continues() {
        exits.push(builder.loop_state());
        code.push(Inst::Break);
    }
    Ok(code)
}
fn expand_initial_code<'a, F>(
    builder: &mut Builder<'a>,
    code: Vec<Inst>,
    breaks: &mut impl Iterator<Item = LoopState>,
    continues: &mut impl Iterator<Item = LoopState>,
    exits: &mut Vec<LoopState>,
    flow: &mut Flow,
    continuation: &mut F,
) -> Result<Vec<Inst>>
where
    F: FnMut(&mut Builder<'a>, Option<Value>) -> Result<()>,
{
    let mut result = Vec::new();
    for mut instruction in code {
        match &mut instruction {
            Inst::Break => exits.push(breaks.next().unwrap()),
            Inst::Continue => {
                result.extend(compile_initial_continuation(
                    builder,
                    continues.next().unwrap(),
                    exits,
                    flow,
                    continuation,
                )?);
                continue;
            }
            Inst::If { yes, no, .. } => {
                *yes = expand_initial_code(
                    builder,
                    std::mem::take(yes),
                    breaks,
                    continues,
                    exits,
                    flow,
                    continuation,
                )?;
                *no = expand_initial_code(
                    builder,
                    std::mem::take(no),
                    breaks,
                    continues,
                    exits,
                    flow,
                    continuation,
                )?;
            }
            Inst::Scope(body) => {
                *body = expand_initial_code(
                    builder,
                    std::mem::take(body),
                    breaks,
                    continues,
                    exits,
                    flow,
                    continuation,
                )?
            }
            _ => {}
        }
        result.push(instruction);
    }
    Ok(result)
}

pub(super) fn initialized_application_word(
    initialized: &BTreeSet<Position>,
    scope: &Position,
    index: ownership::ApplicationIndex,
    word: u32,
) -> bool {
    scope.application_word(Some(index),word).initialized_by(initialized)
}
