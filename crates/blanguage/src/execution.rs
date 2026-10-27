use crate::{Definition, World, ast::*};
use std::collections::{BTreeMap, BTreeSet};

/// 从定义推导的调用参与范围，不是源语言中的函数种类或运行期许可。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Requirement {
    #[default]
    Invocation,
    Domain,
    Host,
}

#[derive(Default)]
struct Body {
    domain: bool,
    local: bool,
    host: bool,
    calls: BTreeSet<String>,
    statements: BTreeSet<String>,
}
impl Body {
    fn call(&mut self, world: &World, d: &Definition, name: &str, statement: bool) {
        if world.resolve(name, &d.module, &d.imports, false).is_some() {
            self.local |= statement || d.function.target == Target::Cpu;
            return;
        }
        if let Some(key) = world.resolve(name, &d.module, &d.imports, true) {
            self.calls.insert(key.clone());
            // A CPU wrapper invokes an already compiled domain through its actual
            // host entry; its body is not part of the caller's stage expansion.
            self.host |= d.function.target == Target::Cpu
                && world.functions[&key].compiled.is_some()
                && world.functions[&key].execution == Requirement::Domain;
            if statement {
                self.statements.insert(key);
            }
        } else if matches!(name, "dataset" | "upload" | "download") {
            self.host = true;
        } else if crate::check::stage_intrinsic(name) {
            self.domain = true;
        } else {
            self.local |= statement || d.function.target == Target::Cpu;
        }
    }
    fn expr(&mut self, world: &World, d: &Definition, e: &Expr, statement: bool) {
        match &e.kind {
            ExprKind::Target(_, call) => {
                self.host = true;
                // target 已拥有发起位置，不把内部域调用传播成调用者的域参数。
                if let ExprKind::Call(_, args) | ExprKind::Specialize(_, _, args) = &call.kind {
                    for arg in args {
                        self.expr(world, d, arg, false);
                    }
                }
            }
            ExprKind::Call(name, args) | ExprKind::Specialize(name, _, args) => {
                self.call(world, d, name, statement);
                for arg in args {
                    self.expr(world, d, arg, false);
                }
            }
            ExprKind::Construct(_, args) | ExprKind::Cooperate(_, _, args) => {
                self.local |= d.function.target == Target::Cpu;
                for arg in args {
                    self.expr(world, d, arg, false);
                }
            }
            ExprKind::Member(e, _) | ExprKind::Unary(_, e) => {
                self.local |= d.function.target == Target::Cpu;
                self.expr(world, d, e, false);
            }
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
                self.local |= d.function.target == Target::Cpu;
                self.expr(world, d, a, false);
                self.expr(world, d, b, false);
            }
            ExprKind::Select(a, b, c) => {
                self.local |= d.function.target == Target::Cpu;
                self.expr(world, d, a, false);
                self.expr(world, d, b, false);
                self.expr(world, d, c, false);
            }
            _ => {}
        }
    }
    fn block(&mut self, world: &World, d: &Definition, body: &[Stmt]) {
        for s in body {
            match &s.kind {
                StmtKind::Parallel(..) | StmtKind::Phase => self.domain = true,
                StmtKind::Expr(e) => self.expr(world, d, e, true),
                StmtKind::For(_, a, b, body) => {
                    // CPU 主流程保留普通循环；设备阶段的常量求值由原阶段编译器负责。
                    self.local |= d.function.target == Target::Cpu;
                    self.expr(world, d, a, false);
                    if let Some(b) = b {
                        self.expr(world, d, b, false);
                    }
                    self.block(world, d, body);
                }
                StmtKind::If(e, a, b) | StmtKind::Match(e, _, a, _, b) => {
                    self.local = true;
                    self.expr(world, d, e, false);
                    self.block(world, d, a);
                    self.block(world, d, b);
                }
                StmtKind::While(e, b) => {
                    self.local = true;
                    self.expr(world, d, e, false);
                    self.block(world, d, b);
                }
                StmtKind::Let(_, _, e) => {
                    self.local = true;
                    self.expr(world, d, e, false);
                }
                StmtKind::Assign(a, _, b) => {
                    self.local = true;
                    self.expr(world, d, a, false);
                    self.expr(world, d, b, false);
                }
                StmtKind::Return(Some(e)) | StmtKind::Seek(_, e) => {
                    self.local = true;
                    self.expr(world, d, e, false);
                }
                StmtKind::Declare(..) | StmtKind::Break | StmtKind::Continue => self.local = true,
                StmtKind::Return(None) => {}
            }
        }
    }
}

pub(crate) fn infer(world: &mut World) {
    let bodies: BTreeMap<_, _> = world
        .functions
        .iter()
        .map(|(key, d)| {
            let mut body = Body::default();
            if d.compiled.is_some() {
                body.domain = d.execution == Requirement::Domain;
                body.host = d.execution == Requirement::Host;
            }
            if d.function.kind == FunctionKind::Value {
                if let Some(stmts) = &d.function.body {
                    body.block(world, d, stmts);
                }
            }
            (key.clone(), body)
        })
        .collect();
    let mut domains: BTreeMap<_, _> = bodies
        .iter()
        .map(|(key, body)| (key.clone(), body.domain))
        .collect();
    loop {
        let mut changed = false;
        for (key, body) in &bodies {
            if !domains[key] && body.calls.iter().any(|callee| domains[callee]) {
                domains.insert(key.clone(), true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // 先求实际域依赖闭包，再判断域外的普通控制，结果不依赖声明／名称顺序。
    let mut hosts: BTreeMap<_, _> = bodies
        .iter()
        .map(|(key, body)| {
            let local = body.local
                || body.statements.iter().any(|callee| !domains[callee])
                || body.calls.iter().any(|callee| {
                    !domains[callee]
                        && (world.functions[key].function.target == Target::Cpu
                            || world.functions[callee].function.target == Target::Cpu)
                });
            (key.clone(), body.host || (domains[key] && local))
        })
        .collect();
    loop {
        let mut changed = false;
        for (key, body) in &bodies {
            if !hosts[key] && body.calls.iter().any(|callee| hosts[callee]) {
                hosts.insert(key.clone(), true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (key, d) in &mut world.functions {
        std::sync::Arc::make_mut(d).execution = if hosts[key] {
            Requirement::Host
        } else if domains[key] {
            Requirement::Domain
        } else {
            Requirement::Invocation
        };
    }
}
