//! 函数类型作用域持有域关系与真实完成效果；模块只交接原输入／输出能力。
use super::*;
pub(super) struct Facts {
    pub(super) scopes: Vec<(BTreeSet<PrefixRelation>, usize)>,
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
                proof.assign_elements(&components, ty.components.len(), &output.project(&components, ty.components.len()));
            }
        }
        for (scope, _) in &mut self.scopes {
            scope.retain(|r| r.count.binding != binding
                || u64::from(r.count.word) >= u64::from(end)
                || u64::from(start) >= u64::from(r.count.word) + 4);
        }
    }
    pub fn holds(&self, relation: PrefixRelation,program:&Program) -> bool {
        self.scopes.last().unwrap().0.iter().any(|known|known.covers(&relation,&program.buffers))
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
        program,
        domain,
        PrefixCount { binding, word },
        &d.source,
        expr.span(),
    )
}
pub(super) fn prove(
    facts: &Facts,
    program: &Program,
    domain: usize,
    count: PrefixCount,
    source: &Source,
    span: Span,
) -> Result<PrefixCount> {
    if facts.holds(PrefixRelation { domain_region: None, domain, count },program) {
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
    let mut selected=data.split('.');let name=selected.next().unwrap();
    let Some(Root::Data(domain, _)) = env.get(name) else {
        return d
            .source
            .fail(contract.span, "B192", "前缀域约束需要命名 dataset 参数");
    };
    let mut domain_ty=&program.buffers[*domain].ty;let mut offset=0;let mut projected=false;
    for field in selected {let Some((_,at,ty))=domain_ty.fields.iter().find(|(name,..)|name==field)else{return d.source.fail(contract.span,"B192","prefix需要实际dataset字段投影");};offset+=*at;domain_ty=ty;projected=true;}
    let (binding, access, count_ty) = collective_root(program, env, count, d, contract.span)?;
    let Some((word, ty)) = count_ty.field(field) else {
        return d.source.fail(contract.span, "B174", "未知集合运算字段");
    };
    if ty != &Ty::scalar(Scalar::U32) {
        return d.source.fail(contract.span, "B175", format!("集合运算字段需要 {:?}", Scalar::U32));
    }
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
    Ok(PrefixRelation { domain_region: projected.then_some((offset,domain_ty.size)),
        domain: *domain,
        count: PrefixCount { binding, word },
    })
}
pub(super) fn relations(
    program: &Program,
    env: &BTreeMap<String, Root>,
    d: &Definition,
    contracts: impl Iterator<Item=FunctionContract>,
    kind: ContractKind,
) -> Result<Vec<PrefixRelation>> {
    let mut result = contracts
        .filter(|c| c.kind == kind && !matches!(c.predicate, ContractPredicate::SameLength { .. } | ContractPredicate::Index { .. } | ContractPredicate::Range { .. }))
        .map(|c| relation(program, env, d, &c))
        .collect::<Result<Vec<_>>>()?;
    result.sort();
    result.dedup();
    Ok(result)
}

pub(super) fn element_requirements(world: &World, buffers: &[Buffer], env: &BTreeMap<String, Root>, instance:&crate::generics::Instance, contracts:impl Iterator<Item=FunctionContract>) -> Result<Vec<ElementFact>> {
    let d=&instance.definition;
    let mut requirements = Vec::new();
    for contract in contracts {
        let value = match &contract.predicate {
            ContractPredicate::Index { value, .. } | ContractPredicate::Range { value, .. } => value,
            _ => continue,
        };
        if contract.kind != ContractKind::Requires {
            return d.source.fail(contract.span, "B225", "整域index／range声明实际输入前提；输出由完整真实写入推导");
        }
        let Some((root, fields)) = value.split_once("[*]") else {
            return d.source.fail(contract.span, "B225", "整域index／range需要实际dataset共同元素的[*]字段选择");
        };
        let Some(Root::Data(binding, access)) = env.get(root) else {
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
        match &contract.predicate {
            ContractPredicate::Index { resource, .. } => {
                if !ty.is_numeric() || ty.components.iter().any(|(scalar, _)| *scalar != Scalar::U32) {
                    return d.source.fail(contract.span, "B225", "index字段需要u32标量或向量分量");
                }
                let Some(Root::Data(target, _)) = env.get(resource) else {
                    return d.source.fail(contract.span, "B225", "整域index需要独立dataset目标");
                };
                requirements.extend(components.into_iter().map(|component| ElementFact::Index {
                    binding: *binding, offset: buffers[*binding].ty.components[component].1, target: *target,
                }));
            }
            ContractPredicate::Range { intervals, .. } => {
                let scalar = bounds::range_scalar(&ty, &d.source, contract.span)?;
                let constants = outputs::range_constants(world,instance.into(),&LexicalScope::Function(instance.clone()),Some(scalar),intervals,contract.span)?;
                let intervals = bounds::numeric_intervals(scalar, constants.iter().map(|(a,b)| (a.bits[0],b.bits[0])), &d.source, contract.span)?;
                requirements.extend(components.into_iter().map(|component| ElementFact::Range(ElementRange {
                    binding: *binding, offset: buffers[*binding].ty.components[component].1, scalar, intervals: intervals.clone(), known_bits:(0,0),
                })));
            }
            _ => unreachable!(),
        }
    }
    requirements.sort();
    requirements.dedup();
    Ok(requirements)
}
impl Facts {
    pub fn assume_elements(&mut self, program: &Program) {
        for requirement in &program.element_requirements {
            let (binding, offset) = match requirement {
                ElementFact::Range(range) => (range.binding, range.offset),
                ElementFact::Index { binding, offset, .. } => (*binding, *offset),
            };
            let component = program.buffers[binding].ty.components.iter().position(|(_, word)| *word == offset).unwrap();
            let proof = self.elements.entry(binding).or_default();
            match requirement {
                ElementFact::Range(range) => {
                    proof.constrain_range(component,&range.intervals);
                    if range.known_bits.0!=0{proof.mark_domain(component,IndexDomain::KnownBits(range.scalar,range.known_bits.0,range.known_bits.1));}
                },
                ElementFact::Index { target, .. } => proof.mark_domain(component, IndexDomain::Data(*target)),
            }
        }
    }
    pub fn element_facts(&self, program: &Program) -> Vec<ElementFact> {
        let mut facts = Vec::new();
        for (&binding, proof) in &self.elements {
            facts.extend(proof.element_guarantees(binding, &program.buffers[binding].ty, |_, _| true));
        }
        facts.sort();
        facts.dedup();
        facts
    }
    pub fn prove_elements(&self, program: &Program, required: &[ElementFact], source: &Source, span: Span) -> Result<()> {
        for requirement in required {
            let (binding, offset) = match requirement {
                ElementFact::Range(range) => (range.binding, range.offset),
                ElementFact::Index { binding, offset, .. } => (*binding, *offset),
            };
            if program.buffers[binding].count == Some(0) { continue; }
            let component = program.buffers[binding].ty.components.iter().position(|(_, word)| *word == offset).unwrap();
            let proven = self.elements.get(&binding).is_some_and(|proof| match requirement {
                ElementFact::Range(range) => proof.holds_range(component, range.scalar, &range.intervals,range.known_bits),
                ElementFact::Index { target, .. } => proof.domains(component).into_iter().any(|domain| matches!(domain, IndexDomain::Data(other) if program.same_length(other, *target)))
                    || proof.element_ranges(binding, &program.buffers[binding].ty).iter().any(|range| range.offset == offset
                        && range.intervals.iter().all(|&(_, upper)| upper < u64::from(program.buffers[*target].count.unwrap_or(0)))),
            });
            if !proven { return source.fail(span, "B226", "整域调用前尚未证明共同字段的数值／目标索引前提"); }
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
    if let ExprKind::Call(name, args) = expr.kind() {
        if name == "prefix" {
            if args.len() != 2 {
                return d
                    .source
                    .fail(expr.span(), "B194", "集合前缀域需要 dataset 和计数字段");
            }
            let value = args.at(0);
            if let ExprKind::Name(name) = value.kind() {
                if let Some(Root::Data(domain, _)) = env.get(name) {
                    let count = resolve(program, env, *domain, &args.at(1), d, facts)?;
                    return Ok((value.clone(), Some(count)));
                }
            }
            return d
                .source
                .fail(args.at(0).span(), "B194", "集合前缀域需要命名 dataset");
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
            .fail(expr.span(), "B175", format!("集合运算字段需要 {scalar:?}"));
    }
    Ok((binding, word, access, prefix))
}
pub(super) fn typed_field(
    program: &Program, env: &BTreeMap<String, Root>, expr: &Expr, d: &Definition, facts: &Facts,
) -> Result<(usize, u32, Access, Option<PrefixCount>, Ty)> {
    if !matches!(expr.kind(), ExprKind::Member(..)) {
        return d.source.fail(expr.span(), "B172", "集合输入需要 dataset.field 或 prefix(dataset,count.field).field");
    }
    let (binding, word, access, ty, prefix) = collective_projection(expr, d, expr.span(), |base| {
        let (value, prefix) = input(program, env, base, d, facts)?;
        let ExprKind::Name(name) = value.kind() else {
            return d.source.fail(expr.span(), "B172", "集合运算参数必须是命名数据集字段");
        };
        let (binding, access, ty) = collective_root(program, env, name, d, expr.span())?;
        Ok((binding, 0, access, ty, prefix))
    })?;
    let access = if access == Access::Write
        && ty.components.iter().all(|(_, offset)| facts.initialized(program, binding, word + offset))
    { Access::InOut } else { access };
    Ok((binding, word, access, prefix, ty.clone()))
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

// A Count field records which original resource projections have a valid prefix.
// These are compile-time facts, never access rights or runtime owners.
pub(super) fn domain(data: &application::DataReference) -> view::Borrow {
    view::Borrow::projection(view::resource_borrow(&data.data), data.projection.clone())
}
pub(super) fn common(a: &view::Borrow, b: &view::Borrow) -> view::Borrow {
    let mut out=view::Borrow::default();
    for (source,when) in a.sources.entries() {
        if b.sources.contains(source) {
            out.sources.insert_when(source.clone(),bounds::Clause::both(&when,&b.sources.when(source).assume(&bounds::Clause::default(),true)));
        }
    }
    for (input,when) in a.inputs.entries() {
        if b.inputs.contains(input) { out.inputs.insert_when(*input,bounds::Clause::both(&when,&b.inputs.when(input).assume(&bounds::Clause::default(),true))); }
    }
    for (call,when) in a.calls.entries() {
        if b.calls.contains(call) { out.calls.insert_when(call.clone(),bounds::Clause::both(&when,&b.calls.when(call).assume(&bounds::Clause::default(),true))); }
    }
    out
}
pub(super) fn covers(known: &view::Borrow, required: &view::Borrow, path: &bounds::Clause) -> bool {
    fn source_covers(known:&view::Source,required:&view::Source,path:&bounds::Clause)->bool {
        if matches!(known,view::Source::Unresolved|view::Source::External|view::Source::Inactive)
            ||matches!(required,view::Source::Unresolved|view::Source::External|view::Source::Inactive){return false;}
        if known==required{return true;}
        match required {
            view::Source::Projection{parent,fields} => match known {
                view::Source::Projection{parent:other,fields:prefix} =>
                    fields.starts_with(prefix)&&covers(other,parent,path),
                _ => covers(&view::Borrow::source(known.clone()),parent,path),
            },
            _ => false,
        }
    }
    fn implied(available:bounds::Clause,when:bounds::Clause,path:&bounds::Clause)->bool {
        bounds::Condition::guard(available).truth_in(&bounds::Clause::both(path,&when))==Some(true)
    }
    if required.empty(){return false;}
    required.sources.entries().all(|(source,when)| {
        let available=known.sources.entries().filter(|(candidate,_)|source_covers(candidate,source,path))
            .fold(bounds::Clause::impossible(),|all,(_,when)|bounds::Clause::either(&all,&when));
        implied(available,when,path)
    }) && required.inputs.entries().all(|(input,when)|implied(known.inputs.when(input).assume(&bounds::Clause::default(),true),when,path))
      && required.calls.entries().all(|(call,when)|implied(known.calls.when(call).assume(&bounds::Clause::default(),true),when,path))
}


pub(super) type Fields = BTreeMap<u32, view::Borrow>;
pub(super) fn join_fields(a:&Fields,b:&Fields,choice:Option<&bounds::Condition>)->Fields {
    a.keys().chain(b.keys()).copied().collect::<BTreeSet<_>>().into_iter().filter_map(|word| {
        let empty=view::Borrow::default();
        let a=a.get(&word).unwrap_or(&empty);let b=b.get(&word).unwrap_or(&empty);
        let known=match choice {Some(choice)=>view::Borrow::selected(choice,a,b),None=>common(a,b)};
        (!known.empty()).then_some((word,known))
    }).collect()
}
