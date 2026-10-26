//! 函数退出在 innermost loop 合流点传递 SSA 结果，再离开函数的 switch 域。
use super::*;

pub(super) struct ReturnScope {
    pub merge: u32,
    pub loops: Vec<ReturnLoop>,
}
pub(super) struct ReturnLoop {
    pub merge: u32,
    pub edges: Vec<(u32, bool)>,
}
fn contains_exit(code: &[Inst]) -> bool {
    code.iter().any(|i| match i {
        Inst::Exit => true,
        Inst::If { yes, no, .. } => contains_exit(yes) || contains_exit(no),
        Inst::Loop {
            condition, body, ..
        } => contains_exit(condition) || contains_exit(body),
        _ => false, // 嵌套 Scope 的 Exit 属于另一个函数。
    })
}
impl Emitter<'_> {
    pub(super) fn function_scope(&mut self, body: &[Inst]) -> Result<(), &'static str> {
        let merge = self.id();
        let begin = self.id();
        let selector = self.constant(Scalar::U32, 0);
        self.emit(247, &[merge, 0]);
        self.emit(251, &[selector, begin]);
        self.emit(248, &[begin]);
        self.return_scopes.push(ReturnScope {
            merge,
            loops: Vec::new(),
        });
        if self.block(body, None)? {
            self.emit(249, &[merge]);
        }
        self.return_scopes.pop();
        self.emit(248, &[merge]);
        Ok(())
    }
    pub(super) fn function_exit(&mut self) {
        let label = self.current_label;
        let scope = self.return_scopes.last_mut().unwrap();
        let target = if let Some(cycle) = scope.loops.last_mut() {
            cycle.edges.push((label, true));
            cycle.merge
        } else {
            scope.merge
        };
        self.emit(249, &[target]);
    }
    pub(super) fn begin_return_loop(&mut self, body: &[Inst], merge: u32) -> bool {
        if !contains_exit(body) {
            return false;
        }
        if let Some(scope) = self.return_scopes.last_mut() {
            scope.loops.push(ReturnLoop {
                merge,
                edges: Vec::new(),
            });
            true
        } else {
            false
        }
    }
    pub(super) fn loop_exit_edge(&mut self, merge: u32, label: u32) {
        if let Some(cycle) = self
            .return_scopes
            .last_mut()
            .and_then(|s| s.loops.last_mut())
        {
            if cycle.merge == merge {
                cycle.edges.push((label, false));
            }
        }
    }
    pub(super) fn finish_return_loop(&mut self) {
        let cycle = self.return_scopes.last_mut().unwrap().loops.pop().unwrap();
        let value = self.id();
        let mut operands = vec![self.types[&Scalar::Bool], value];
        for (label, returned) in cycle.edges {
            let constant = self.constant(Scalar::Bool, u32::from(returned));
            operands.extend([constant, label]);
        }
        self.emit(245, &operands);
        let label = self.current_label;
        let scope = self.return_scopes.last_mut().unwrap();
        let target = if let Some(parent) = scope.loops.last_mut() {
            parent.edges.push((label, true));
            parent.merge
        } else {
            scope.merge
        };
        let next = self.id();
        self.emit(247, &[next, 0]);
        self.emit(250, &[value, target, next]);
        self.emit(248, &[next]);
    }
}
