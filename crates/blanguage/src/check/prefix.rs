//! 函数类型作用域持有域关系与真实完成效果；模块只交接原输入／输出能力。
use super::*;
pub(super) struct Facts {
    scopes: Vec<(BTreeSet<PrefixRelation>, usize)>,
    pub outputs: Vec<crate::module::domain::Output>,
    pub elements: BTreeMap<usize, bounds::Proof>,
}
impl Facts {
    pub fn new(relations: &[PrefixRelation]) -> Self {
        Self {
            scopes: vec![(relations.iter().copied().collect(), 0)],
            outputs: Vec::new(),
            elements: BTreeMap::new(),
        }
    }
    pub fn written(&mut self, program: &Program, binding: usize, start: u32, end: u32, output: Option<&bounds::Proof>) {
        let proof = match output {
            Some(_) => Some(self.elements.entry(binding).or_default()),
            None => self.elements.get_mut(&binding),
        };
        if let Some(proof) = proof {
            let ty = &program.buffers[binding].ty;
            let components = ty.components.iter().enumerate()
                .filter_map(|(i, (scalar, offset))| (u64::from(*offset) < u64::from(end)
                    && u64::from(start) < u64::from(*offset) + u64::from(scalar.size())).then_some(i))
                .collect::<Vec<_>>();
            proof.forget_value(&components, ty.components.len());
            if let Some(output) = output {
                proof.assign_ranges(&components, ty.components.len(), &output.project(&components, ty.components.len()));
            }
        }
        for (scope, _) in &mut self.scopes {
            scope.retain(|r| r.count.binding != binding
                || u64::from(r.count.word) >= u64::from(end)
                || u64::from(start) >= u64::from(r.count.word) + 4);
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
                Some(domain) => program.same_length(domain, binding),
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
            "整域接口接受prefix域约束及same_length长度前提；值约束用于普通／cooperative函数",
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
        .filter(|c| c.kind == kind && !matches!(c.predicate, ContractPredicate::SameLength { .. } | ContractPredicate::Index { .. }))
        .map(|c| relation(program, env, d, c))
        .collect::<Result<Vec<_>>>()?;
    result.sort_by_key(|r| (r.domain, r.count.binding, r.count.word));
    result.dedup();
    Ok(result)
}

pub(super) fn element_requirements(buffers: &[Buffer], env: &BTreeMap<String, Root>, d: &Definition) -> Result<Vec<ElementRequirement>> {
    let mut requirements = Vec::new();
    for contract in &d.function.contracts {
        let ContractPredicate::Index { value, resource } = &contract.predicate else { continue; };
        if contract.kind != ContractKind::Requires {
            return d.source.fail(contract.span, "B225", "整域index声明实际输入前提；输出由完整真实写入推导");
        }
        let Some((root, fields)) = value.split_once("[*]") else {
            return d.source.fail(contract.span, "B225", "整域index需要实际dataset共同元素的[*]字段选择");
        };
        let (Some(Root::Data(binding, access)), Some(Root::Data(target, _))) = (env.get(root), env.get(resource)) else {
            return d.source.fail(contract.span, "B225", "整域index需要共同元素源与独立dataset目标");
        };
        if !access.reads() || *access == Access::Atomic {
            return d.source.fail(contract.span, "B225", "共同输入字段需要普通read或inout访问");
        }
        let mut ty = buffers[*binding].ty.clone();
        let mut components: Vec<_> = (0..ty.components.len()).collect();
        for field in fields.trim_start_matches('.').split('.').filter(|field| !field.is_empty()) {
            let (selected, indices) = member_type(&ty, field, &d.source, contract.span)?;
            components = indices.iter().map(|&i| components[i]).collect();
            ty = selected;
        }
        if !ty.is_numeric() || ty.components.iter().any(|(scalar, _)| *scalar != Scalar::U32) {
            return d.source.fail(contract.span, "B225", "index字段需要u32标量或向量分量");
        }
        requirements.extend(components.into_iter().map(|component| ElementRequirement::Index {
            binding: *binding, offset: buffers[*binding].ty.components[component].1, target: *target,
        }));
    }
    requirements.sort();
    requirements.dedup();
    Ok(requirements)
}
impl Facts {
    pub fn assume_elements(&mut self, program: &Program) {
        for requirement in &program.element_requirements {
            if let ElementRequirement::Index { binding, offset, target } = requirement {
                let component = program.buffers[*binding].ty.components.iter().position(|(_, word)| word == offset).unwrap();
                self.elements.entry(*binding).or_default().mark_domain(component, IndexDomain::Data(*target));
            }
        }
    }
    pub fn element_ranges(&self, program: &Program) -> Vec<ElementRange> {
        self.elements.iter().flat_map(|(&binding, proof)| proof.element_ranges(binding, &program.buffers[binding].ty)).collect()
    }
    pub fn prove_elements(&self, program: &Program, required: &[ElementRequirement], d: &Definition, span: Span) -> Result<()> {
        for requirement in required {
            let ElementRequirement::Index { binding, offset, target } = requirement else { continue; };
            if program.buffers[*binding].count == Some(0) { continue; }
            let component = program.buffers[*binding].ty.components.iter().position(|(_, word)| word == offset).unwrap();
            let proven = self.elements.get(binding).is_some_and(|proof| {
                proof.domains(component).into_iter().any(|domain| matches!(domain, IndexDomain::Data(other) if program.same_length(other, *target)))
                    || proof.element_ranges(*binding, &program.buffers[*binding].ty).iter().any(|range| range.offset == *offset
                        && range.intervals.iter().all(|&(_, upper)| upper < u64::from(program.buffers[*target].count.unwrap_or(0))))
            });
            if !proven { return d.source.fail(span, "B226", "整域调用前尚未证明共同字段的目标索引前提"); }
        }
        Ok(())
    }
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
