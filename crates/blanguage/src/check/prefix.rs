//! 函数类型作用域持有域关系与真实完成效果；模块只交接原输入／输出能力。
use super::*;
pub(super) struct Facts {
    scopes: Vec<(BTreeSet<PrefixRelation>, usize)>,
    pub outputs: Vec<crate::module::domain::Output>,
}
impl Facts {
    pub fn new(relations: &[PrefixRelation]) -> Self {
        Self {
            scopes: vec![(relations.iter().copied().collect(), 0)],
            outputs: Vec::new(),
        }
    }
    pub fn written(&mut self, bindings: &[usize]) {
        for (scope, _) in &mut self.scopes {
            scope.retain(|r| !bindings.contains(&r.count.binding));
        }
    }
    pub fn holds(&self, relation: PrefixRelation) -> bool {
        self.scopes.last().unwrap().0.contains(&relation)
    }
    pub fn grant(&mut self, relation: PrefixRelation) {
        self.scopes.last_mut().unwrap().0.insert(relation);
    }
    pub fn enter(&mut self, relations: &[PrefixRelation]) {
        self.scopes
            .push((relations.iter().copied().collect(), self.outputs.len()));
    }
    pub fn leave(&mut self) {
        self.scopes.pop();
    }
    pub fn initialized(&self, program: &Program, binding: usize, word: u32) -> bool {
        let buffer = &program.buffers[binding];
        buffer.count == Some(0) || self.outputs[self.scopes.last().unwrap().1..].iter().any(|output| {
            output.binding == binding && output.words.contains(&word) && match output.domain {
                Some(domain) => domain == binding
                    || buffer.count.is_some() && buffer.count == program.buffers[domain].count,
                None => buffer.count == Some(1),
            }
        })
    }
    pub fn relations(&self) -> Vec<PrefixRelation> {
        self.scopes
            .iter()
            .map(|(scope, _)| scope)
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}
pub(super) fn resolve(
    program: &Program,
    env: &BTreeMap<String, Root>,
    domain: usize,
    expr: &Expr,
    d: &Definition,
    facts: &Facts,
) -> Result<PrefixCount> {
    let (binding, word, _) = collective_field(program, env, expr, d, Scalar::U32)?;
    prove(
        facts,
        domain,
        PrefixCount { binding, word },
        &d.source,
        expr.span,
    )
}
pub(super) fn prove(
    facts: &Facts,
    domain: usize,
    count: PrefixCount,
    source: &Source,
    span: Span,
) -> Result<PrefixCount> {
    if facts.holds(PrefixRelation { domain, count }) {
        return Ok(count);
    }
    source.fail(
        span,
        "B191",
        "prefix 计数缺少当前类型作用域中的生产证明或接口能力，或已被其他阶段改写",
    )
}
pub(super) fn relation(
    program: &Program,
    env: &BTreeMap<String, Root>,
    d: &Definition,
    contract: &FunctionContract,
) -> Result<PrefixRelation> {
    let ContractPredicate::Prefix { data, count, field } = &contract.predicate else {
        return d.source.fail(
            contract.span,
            "B225",
            "program 接口只接受 prefix 域约束；值约束用于普通／cooperative 函数",
        );
    };
    let Some(Root::Data(domain, _)) = env.get(data) else {
        return d
            .source
            .fail(contract.span, "B192", "前缀域约束需要命名 dataset 参数");
    };
    let expr = Expr {
        span: contract.span,
        kind: ExprKind::Member(
            Box::new(Expr {
                span: contract.span,
                kind: ExprKind::Name(count.clone()),
            }),
            field.clone(),
        ),
    };
    let (binding, word, access) = collective_field(program, env, &expr, d, Scalar::U32)?;
    if *domain == binding
        || program.buffers[binding].count != Some(1)
    {
        return d.source.fail(
            contract.span,
            "B192",
            "前缀域约束需要独立形状 1 资源中的 u32 计数字段",
        );
    }
    if contract.kind == ContractKind::Requires && !access.reads() {
        return d.source.fail(
            contract.span,
            "B192",
            "requires prefix 的计数参数需要 read 或 inout 能力",
        );
    }
    Ok(PrefixRelation {
        domain: *domain,
        count: PrefixCount { binding, word },
    })
}
pub(super) fn relations(
    program: &Program,
    env: &BTreeMap<String, Root>,
    d: &Definition,
    kind: ContractKind,
) -> Result<Vec<PrefixRelation>> {
    let mut result = d
        .function
        .contracts
        .iter()
        .filter(|c| c.kind == kind)
        .map(|c| relation(program, env, d, c))
        .collect::<Result<Vec<_>>>()?;
    result.sort_by_key(|r| (r.domain, r.count.binding, r.count.word));
    result.dedup();
    Ok(result)
}

/// 集合输入的 prefix 是域表达式；裸字段／输出仍由既有集合类型规则检查。
pub(super) fn input(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
    facts: &Facts,
) -> Result<(Expr, Option<PrefixCount>)> {
    if let ExprKind::Call(name, args) = &expr.kind {
        if name == "prefix" {
            if args.len() != 2 {
                return d
                    .source
                    .fail(expr.span, "B194", "集合前缀域需要 dataset 和计数字段");
            }
            if let ExprKind::Name(name) = &args[0].kind {
                if let Some(Root::Data(domain, _)) = env.get(name) {
                    let count = resolve(program, env, *domain, &args[1], d, facts)?;
                    return Ok((args[0].clone(), Some(count)));
                }
            }
            return d
                .source
                .fail(args[0].span, "B194", "集合前缀域需要命名 dataset");
        }
    }
    Ok((expr.clone(), None))
}
pub(super) fn field(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
    facts: &Facts,
    scalar: Scalar,
) -> Result<(usize, u32, Access, Option<PrefixCount>)> {
    let (binding, word, access, prefix, ty) = typed_field(program, env, expr, d, facts)?;
    if ty != Ty::scalar(scalar) {
        return d
            .source
            .fail(expr.span, "B175", format!("集合运算字段需要 {scalar:?}"));
    }
    Ok((binding, word, access, prefix))
}
pub(super) fn typed_field(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
    facts: &Facts,
) -> Result<(usize, u32, Access, Option<PrefixCount>, Ty)> {
    let mut fields = Vec::new();
    let mut base = expr;
    while let ExprKind::Member(parent, field) = &base.kind {
        fields.push(field.clone());
        base = parent;
    }
    if fields.is_empty() {
        return d.source.fail(
            expr.span,
            "B172",
            "集合输入需要 dataset.field 或 prefix(dataset,count.field).field",
        );
    }
    let (mut value, prefix) = input(program, env, base, d, facts)?;
    for field in fields.into_iter().rev() {
        value = Expr {
            span: expr.span,
            kind: ExprKind::Member(Box::new(value), field),
        };
    }
    let (binding, word, access, ty) = collective_value_field(program, env, &value, d)?;
    let access = if access == Access::Write
        && ty.components.iter().all(|(_, offset)| facts.initialized(program, binding, word + offset))
    { Access::InOut } else { access };
    Ok((binding, word, access, prefix, ty))
}

/// 集合阶段只读取源字段并写目标的真实分量；同一独占记录的不同字不发生别名。
pub(super) fn fields_overlap(
    source: usize,
    source_word: u32,
    source_ty: &Ty,
    target: usize,
    target_word: u32,
    target_ty: &Ty,
) -> bool {
    source == target
        && source_ty.components.iter().any(|(_, a)| {
            target_ty
                .components
                .iter()
                .any(|(_, b)| source_word + *a == target_word + *b)
        })
}
