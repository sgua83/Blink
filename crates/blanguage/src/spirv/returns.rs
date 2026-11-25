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
pub(super) unsafe extern "C" fn structured_constant(
    context: StructuredContext, kind: ControlConstant, value: u32,
) -> u32 {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    let scalar = match kind { ControlConstant::Selector => Scalar::U32, ControlConstant::Flag => Scalar::Bool };
    emitter.constant(scalar, value)
}

pub(super) unsafe extern "C" fn structured_open_function(context: StructuredContext, merge: u32) {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.return_scopes.push(ReturnScope { merge, loops: Vec::new() });
}

pub(super) unsafe extern "C" fn structured_close_function(context: StructuredContext) {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.return_scopes.pop();
}

pub(super) unsafe extern "C" fn structured_exit_targets(context: StructuredContext) -> ReturnTargets {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    let label = emitter.current_label;
    let scope = emitter.return_scopes.last_mut().unwrap();
    let (loop_merge, loop_edges) = scope.loops.last_mut().map_or((0, std::ptr::null_mut()), |cycle| {
        (cycle.merge, std::ptr::from_mut(&mut cycle.edges).cast())
    });
    ReturnTargets { function_merge: scope.merge, loop_merge, loop_edges, label }
}

pub(super) unsafe extern "C" fn structured_record_return(edges: *mut std::ffi::c_void, label: u32) {
    unsafe { &mut *edges.cast::<Vec<ReturnEdge>>() }.push(ReturnEdge { label, returned: 1 });
}

impl Emitter<'_> {
    pub(super) fn begin_return_loop(&mut self, body: &[Inst], merge: u32) -> bool {
        let Some(scope) = self.return_scopes.last_mut() else { return false; };
        if !crate::ir::dataflow::has_function_exit(body) {
            return false;
        }
        scope.loops.push(ReturnLoop {
            merge,
            edges: Vec::new(),
        });
        true
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
        let bool_type = self.types[&Scalar::Bool];
        let mut emission = StructuredEmission { emitter: self, error: None };
        unsafe {
            lower_return_loop(cycle.edges.as_ptr(), cycle.edges.len() as u32, bool_type, emission.host());
        }
    }
}
