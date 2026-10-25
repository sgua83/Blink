//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;

fn assigned_root(expr: &Expr, names: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Name(name) => {
            names.insert(name.clone());
        }
        ExprKind::Member(base, _) | ExprKind::Index(base, _) => assigned_root(base, names),
        _ => {}
    }
}
pub(super) fn expression_assignments(
    world: &World,
    definition: &Definition,
    expr: &Expr,
    names: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::Call(name, args)
        | ExprKind::Cooperate(name, _, args)
        | ExprKind::Specialize(name, _, args) => {
            let record = matches!(expr.kind, ExprKind::Specialize(..))
                && world
                    .resolve(name, &definition.module, &definition.imports, false)
                    .is_some();
            if !record
                && let Some(key) =
                    world.resolve(name, &definition.module, &definition.imports, true)
            {
                for (param, arg) in world.functions[&key].function.params.iter().zip(args) {
                    if matches!(param.access, Access::InOut | Access::Write)
                        && !param.ty.dataset
                        && !param.ty.tile
                    {
                        assigned_root(arg, names);
                    }
                }
            }
            for arg in args {
                expression_assignments(world, definition, arg, names);
            }
        }
        ExprKind::Construct(_, args) => {
            for arg in args {
                expression_assignments(world, definition, arg, names);
            }
        }
        ExprKind::Member(base, _) | ExprKind::Unary(_, base) => {
            expression_assignments(world, definition, base, names)
        }
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            expression_assignments(world, definition, a, names);
            expression_assignments(world, definition, b, names);
        }
        ExprKind::Select(a, b, c) => {
            expression_assignments(world, definition, a, names);
            expression_assignments(world, definition, b, names);
            expression_assignments(world, definition, c, names);
        }
        _ => {}
    }
}
pub(super) fn assigned_names(
    world: &World,
    definition: &Definition,
    code: &[Stmt],
    names: &mut BTreeSet<String>,
) {
    for s in code {
        match &s.kind {
            StmtKind::Assign(lhs, _, rhs) => {
                assigned_root(lhs, names);
                expression_assignments(world, definition, lhs, names);
                expression_assignments(world, definition, rhs, names);
            }
            StmtKind::Let(_, _, expr)
            | StmtKind::Expr(expr)
            | StmtKind::Return(Some(expr))
            | StmtKind::Seek(_, expr) => expression_assignments(world, definition, expr, names),
            StmtKind::If(expr, a, b) => {
                expression_assignments(world, definition, expr, names);
                assigned_names(world, definition, a, names);
                assigned_names(world, definition, b, names);
            }
            StmtKind::While(expr, body) => {
                expression_assignments(world, definition, expr, names);
                assigned_names(world, definition, body, names);
            }
            StmtKind::For(_, start, end, body) => {
                expression_assignments(world, definition, start, names);
                if let Some(end) = end {
                    expression_assignments(world, definition, end, names);
                }
                assigned_names(world, definition, body, names);
            }
            StmtKind::Parallel(_, domain, body) => {
                if let Some(count) = &domain.count {
                    expression_assignments(world, definition, count, names);
                }
                if let Some(selection) = &domain.selection {
                    expression_assignments(world, definition, selection, names);
                }
                assigned_names(world, definition, body, names);
            }
            _ => {}
        }
    }
}
pub(super) fn modified_owners(
    env: &BTreeMap<String, Binding>,
    names: &BTreeSet<String>,
) -> BTreeSet<String> {
    let data: BTreeSet<_> = names
        .iter()
        .filter_map(|name| {
            if let Some(Binding::Data(view)) = env.get(name) {
                Some(view.binding)
            } else {
                None
            }
        })
        .collect();
    env.iter()
        .filter_map(|(name, binding)| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            let through_data =
                matches!(&owner.place, Place::Element { binding, .. } if data.contains(binding));
            (names.contains(name) || through_data).then(|| name.clone())
        })
        .collect()
}
