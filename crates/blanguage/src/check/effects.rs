//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;

type ComponentProjection = (Ty, Vec<Vec<usize>>);

impl Builder<'_> {
    // 只投影可能改写的现有所有者，不求值下标或再次生成表达式代码。
    fn written_projection(
        &self,
        expr: &Expr,
        name: &str,
        owner: &OwnedBinding,
    ) -> Result<Option<ComponentProjection>> {
        match &expr.kind {
            ExprKind::Name(root) if root == name => {
                let ty = owner.place.ty().clone();
                let components = (0..ty.components.len()).map(|i| vec![i]).collect();
                Ok(Some((ty, components)))
            }
            ExprKind::Member(base, field) => {
                let Some((ty, components)) = self.written_projection(base, name, owner)? else {
                    return Ok(None);
                };
                let (ty, selected) = self.member(&ty, field, expr.span)?;
                Ok(Some((
                    ty,
                    selected.iter().map(|i| components[*i].clone()).collect(),
                )))
            }
            ExprKind::Index(base, index) => {
                if let ExprKind::Name(root) = &base.kind
                    && let Some(Binding::Data(view)) = self.env.get(root)
                {
                    if let Place::Element { binding, words, .. } = &owner.place
                        && *binding == view.binding
                    {
                        let ty = self.program.buffers[*binding].ty.clone();
                        let components = ty
                            .components
                            .iter()
                            .map(|(_, offset)| {
                                words
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, word)| (*word == offset / 4).then_some(i))
                                    .collect()
                            })
                            .collect();
                        return Ok(Some((ty, components)));
                    }
                    return Ok(None);
                }
                let Some((ty, components)) = self.written_projection(base, name, owner)? else {
                    return Ok(None);
                };
                let Some(element) = &ty.array else {
                    return Ok(None);
                };
                let constant = match &index.kind {
                    ExprKind::StaticU32(value) => Some(*value),
                    ExprKind::Number(text) => {
                        let (ty, value) = number(&self.definition.source, index.span, text)?;
                        (ty == Ty::scalar(Scalar::U32)).then_some(value)
                    }
                    ExprKind::Name(name) => match self.env.get(name) {
                        Some(Binding::Value(value)) if value.ty == Ty::scalar(Scalar::U32) => {
                            value.constant.as_ref().map(|bits| bits[0])
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let width = element.components.len();
                let elements: Vec<_> = (0..ty.values[0] as usize)
                    .filter(|i| constant.is_none_or(|value| *i == value as usize))
                    .collect();
                let selected = (0..width)
                    .map(|component| {
                        elements
                            .iter()
                            .flat_map(|i| components[*i * width + component].iter().copied())
                            .collect()
                    })
                    .collect();
                Ok(Some(((**element).clone(), selected)))
            }
            _ => Ok(None),
        }
    }
    fn invalidate_writes(&mut self, writes: &[&Expr], borrowed_data: Option<&str>) -> Result<()> {
        let resources: Vec<_> = self
            .env
            .iter()
            .filter_map(|(name, binding)| {
                let Binding::ApplicationData(data) = binding else {
                    return None;
                };
                (borrowed_data != Some(name.as_str())
                    && !data
                        .handle
                        .proof
                        .known_identity(0)
                        .is_some_and(|id| self.application_borrowed(id))
                    && writes
                        .iter()
                        .any(|expr| matches!(&expr.kind, ExprKind::Name(target) if target == name)))
                .then(|| name.clone())
            })
            .collect();
        for name in resources {
            if let Some(Binding::ApplicationData(data)) = self.env.get_mut(&name) {
                data.handle.proof = bounds::Proof::default();
                data.length.proof = data
                    .count
                    .map(|count| bounds::Proof::unsigned(count, count))
                    .unwrap_or_default();
                data.length.constant = None;
                data.length.uniform = false;
            }
        }
        for (name, binding) in &mut self.env {
            if let Binding::OwnedResult(result) = binding
                && writes
                    .iter()
                    .any(|e| matches!(&e.kind,ExprKind::Name(target) if target==name))
            {
                result.invalidate();
            }
        }
        let mut modified: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (name, binding) in &self.env {
            if let Binding::Owned(owner) = binding {
                for expr in writes {
                    if let Some((_, components)) = self.written_projection(expr, name, owner)? {
                        for component in components.into_iter().flatten() {
                            modified.entry(name.clone()).or_default().insert(component);
                        }
                    }
                }
            }
        }
        for (name, binding) in &mut self.env {
            if let Binding::Owned(owner) = binding
                && let Some(components) = modified.get(name)
            {
                owner.uniform = false;
                owner.proof.assign(
                    &components.iter().copied().collect::<Vec<_>>(),
                    owner.place.ty().components.len(),
                    bounds::Proof::default(),
                );
            }
        }
        Ok(())
    }
    pub(super) fn invalidate_expression(&mut self, expr: &Expr) -> Result<()> {
        let mut writes = Vec::new();
        expression_writes(self.world, &self.definition, expr, &mut |place| {
            writes.push(place)
        });
        self.invalidate_writes(&writes, None)
    }
    pub(super) fn invalidate_assigned(
        &mut self,
        body: &[Stmt],
        borrowed_data: Option<&str>,
    ) -> Result<()> {
        let mut writes = Vec::new();
        statement_writes(self.world, &self.definition, body, &mut |place| {
            writes.push(place)
        });
        self.invalidate_writes(&writes, borrowed_data)
    }
}

fn assigned_root(expr: &Expr, names: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Name(name) => {
            names.insert(name.clone());
        }
        ExprKind::Member(base, _) | ExprKind::Index(base, _) => assigned_root(base, names),
        _ => {}
    }
}
fn expression_writes<'e>(
    world: &World,
    definition: &Definition,
    expr: &'e Expr,
    writes: &mut impl FnMut(&'e Expr),
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
                        writes(arg);
                    }
                }
            }
            for arg in args {
                expression_writes(world, definition, arg, writes);
            }
        }
        ExprKind::Construct(_, args) => {
            for arg in args {
                expression_writes(world, definition, arg, writes);
            }
        }
        ExprKind::Member(base, _) | ExprKind::Unary(_, base) => {
            expression_writes(world, definition, base, writes)
        }
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            expression_writes(world, definition, a, writes);
            expression_writes(world, definition, b, writes);
        }
        ExprKind::Select(a, b, c) => {
            expression_writes(world, definition, a, writes);
            expression_writes(world, definition, b, writes);
            expression_writes(world, definition, c, writes);
        }
        _ => {}
    }
}
fn statement_writes<'e>(
    world: &World,
    definition: &Definition,
    code: &'e [Stmt],
    writes: &mut impl FnMut(&'e Expr),
) {
    for s in code {
        match &s.kind {
            StmtKind::Assign(lhs, _, rhs) => {
                writes(lhs);
                expression_writes(world, definition, lhs, writes);
                expression_writes(world, definition, rhs, writes);
            }
            StmtKind::Let(_, _, expr)
            | StmtKind::Expr(expr)
            | StmtKind::Return(Some(expr))
            | StmtKind::Seek(_, expr) => expression_writes(world, definition, expr, writes),
            StmtKind::If(expr, a, b) | StmtKind::Match(expr, _, a, _, b) => {
                expression_writes(world, definition, expr, writes);
                statement_writes(world, definition, a, writes);
                statement_writes(world, definition, b, writes);
            }
            StmtKind::While(expr, body) => {
                expression_writes(world, definition, expr, writes);
                statement_writes(world, definition, body, writes);
            }
            StmtKind::For(_, start, end, body) => {
                expression_writes(world, definition, start, writes);
                if let Some(end) = end {
                    expression_writes(world, definition, end, writes);
                }
                statement_writes(world, definition, body, writes);
            }
            StmtKind::Parallel(_, domain, body) => {
                if let Some(count) = &domain.count {
                    expression_writes(world, definition, count, writes);
                }
                if let Some(selection) = &domain.selection {
                    expression_writes(world, definition, selection, writes);
                }
                statement_writes(world, definition, body, writes);
            }
            _ => {}
        }
    }
}
pub(super) fn expression_assignments(
    world: &World,
    definition: &Definition,
    expr: &Expr,
    names: &mut BTreeSet<String>,
) {
    expression_writes(world, definition, expr, &mut |place| {
        assigned_root(place, names)
    });
}
pub(super) fn assigned_names(
    world: &World,
    definition: &Definition,
    code: &[Stmt],
    names: &mut BTreeSet<String>,
) {
    statement_writes(world, definition, code, &mut |place| {
        assigned_root(place, names)
    });
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
