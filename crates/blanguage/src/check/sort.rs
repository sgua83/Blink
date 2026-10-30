//! 稳定排序按不可变键建立唯一目标序号；源记录与容量尾部保持原值。
use super::*;
fn key<'a>(expr: &'a Expr, d: &Definition) -> Result<(&'a Expr, SortOrder)> {
    if let ExprKind::Call(name, args) = &expr.kind
        && name == "descending"
    {
        if args.len() != 1 {
            return d
                .source
                .fail(expr.span, "B169", "descending 需要一个排序键字段");
        }
        return Ok((&args[0], SortOrder::Descending));
    }
    Ok((expr, SortOrder::Ascending))
}
pub(super) fn compile(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &[Expr],
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
) -> Result<()> {
    if args.len() < 2 {
        return d.source.fail(
            span,
            "B169",
            "stable_sort_by_key 需要按优先级排列的数值源键字段和输出 dataset",
        );
    }
    let (first, _) = key(&args[0], d)?;
    let (source, _, _, prefix, _) = prefix::typed_field(program, env, first, d, facts)?;
    let mut keys = Vec::new();
    for arg in &args[..args.len() - 1] {
        let (field, order) = key(arg, d)?;
        let (binding, word, access, domain, ty) =
            prefix::typed_field(program, env, field, d, facts)?;
        if binding != source || domain != prefix || !matches!(access, Access::Read | Access::InOut)
            || !env.values().any(|root| matches!(root, Root::Data(binding, Access::Read | Access::InOut) if *binding == source))
        {
            return d
                .source
                .fail(arg.span, "B170", "排序键必须来自同一 read 输入和同一有效域");
        }
        if !ty.is_numeric() {
            return d.source.fail(
                arg.span,
                "B175",
                "排序键需要数值字段；bool、记录、数组和资源不能作为键",
            );
        }
        for &(scalar, offset) in &ty.components {
            if scalar.size() == 8 {
                let high = if scalar.is_float() {
                    Scalar::F32
                } else if scalar.is_signed() {
                    Scalar::I32
                } else {
                    Scalar::U32
                };
                keys.push((word + offset + 4, high, order));
            }
            keys.push((word + offset, scalar, order));
        }
    }
    let output = args.last().unwrap();
    let (target, write) = compact::data(env, output, d)?;
    if !write.writes() || source == target || prefix.is_some_and(|p| p.binding == target) {
        return d.source.fail(
            span,
            "B170",
            "排序需要独立 read 输入和可写输出，输出不得改写输入域计数",
        );
    }
    if program.buffers[source].ty != program.buffers[target].ty {
        return d
            .source
            .fail(output.span, "B189", "排序输入／输出的完整记录类型必须相同");
    }
    if let (Some(a), Some(b)) = (program.buffers[source].count, program.buffers[target].count) {
        if b < a {
            return d
                .source
                .fail(span, "B188", "排序输出容量必须容纳完整输入域");
        }
    }
    facts.written(&[target]);
    if let Some(count) = prefix {
        facts.grant(PrefixRelation {
            domain: target,
            count,
        });
    }
    let reads: Vec<_> = std::iter::once(source)
        .chain(prefix.map(|p| p.binding))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    program.stages.push(Stage {
        tiles: Vec::new(),
        domain: source,
        prefix,
        selection: None,
        operation: Operation::StableSort(Sorting {
            source,
            keys,
            target,
        }),
        boundary_before: true,
        registers: Vec::new(),
        locals: Default::default(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target],
        cross_access: reads,
    });
    Ok(())
}
