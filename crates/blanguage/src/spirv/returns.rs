//! 函数退出在 innermost loop 合流点传递 SSA 结果，再离开函数的 switch 域。
use super::*;

pub(super) struct ReturnScope {
    pub merge: u32,
    pub loops: Vec<ReturnLoop>,
}
pub(super) struct ReturnLoop {
    pub merge: u32,
    pub edges: Vec<ReturnEdge>,
}
#[repr(C)]
pub(super) struct ReturnEdge {
    label: u32,
    returned: u32,
}
fn contains_exit(code: &[Inst]) -> bool {
    code.iter().any(|i| match &(i).kind {
        InstKind::Exit => true,
        InstKind::If { yes, no, .. } => contains_exit(yes) || contains_exit(no),
        InstKind::Loop {
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
        self.emit_control(8, |body| unsafe { open_function_scope(merge, selector, begin, body) });
        self.return_scopes.push(ReturnScope {
            merge,
            loops: Vec::new(),
        });
        let reaches = u32::from(self.block(body, None)?);
        self.return_scopes.pop();
        self.emit_control(4, |body| unsafe { next_control_block(merge, merge, reaches, body) });
        Ok(())
    }
    pub(super) fn function_exit(&mut self) -> bool {
        let label = self.current_label;
        let scope = self.return_scopes.last_mut().unwrap();
        let target = if let Some(cycle) = scope.loops.last_mut() {
            cycle.edges.push(ReturnEdge { label, returned: 1 });
            cycle.merge
        } else {
            scope.merge
        };
        self.leave_control(target)
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
                cycle.edges.push(ReturnEdge { label, returned: 0 });
            }
        }
    }
    pub(super) fn finish_return_loop(&mut self) {
        let cycle = self.return_scopes.last_mut().unwrap().loops.pop().unwrap();
        let value = self.id();
        let mut constants = [0; 2];
        for edge in &cycle.edges {
            constants[edge.returned as usize] = self.constant(Scalar::Bool, edge.returned);
        }
        let label = self.current_label;
        let scope = self.return_scopes.last_mut().unwrap();
        let target = if let Some(parent) = scope.loops.last_mut() {
            parent.edges.push(ReturnEdge { label, returned: 1 });
            parent.merge
        } else {
            scope.merge
        };
        let next = self.id();
        let input = LoopReturn {
            bool_type: self.types[&Scalar::Bool],
            result: value,
            target,
            merge: next,
            not_returned: constants[0],
            returned: constants[1],
            edge_count: cycle.edges.len() as u32,
        };
        self.emit_control(12 + 2 * cycle.edges.len(), |body| unsafe {
            dispatch_loop_return(input, cycle.edges.as_ptr(), body)
        });
    }
}
