//! 全域归约拥有一个输出元素，各分量沿用同一数值类型与集合算子。
use super::*;
pub(super) fn compile(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    operator: CollectiveOperator,
    args: &[Expr],
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
) -> Result<()> {
    if args.len() != 2 {
        return d.source.fail(span, "B169", "归约需要源数值字段和输出字段");
    }
    let (source, source_word, read, prefix, ty) =
        prefix::typed_field(program, env, &args[0], d, facts)?;
    let (target, target_word, write, target_ty) =
        collective_value_field(program, env, &args[1], d)?;
    if !ty.is_numeric() || ty != target_ty || !operator.accepts(ty.components[0].0) {
        return d.source.fail(
            span,
            "B175",
            "归约需要相同类型的数值标量／向量字段；按位归约只接受整数",
        );
    }
    if prefix::fields_overlap(source, source_word, &ty, target, target_word, &target_ty)
        || !matches!(read, Access::Read | Access::InOut)
        || !write.writes()
        || prefix.is_some_and(|p| p.binding == target)
    {
        return d.source.fail(
            span,
            "B170",
            "归约需要可读输入和可写输出，实际字段不能重叠或改写输入域计数",
        );
    }
    if program.buffers[target].count != Some(1) {
        return d.source.fail(span, "B171", "归约输出形状必须为 1");
    }
    facts.written(&[target]);
    let reads: Vec<_> = std::iter::once(source)
        .chain(prefix.map(|p| p.binding))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    program.stages.push(Stage {
        domain: source,
        prefix,
        selection: None,
        operation: Operation::Reduce(ReductionSpec {
            fields: Reduction {
                source,
                source_word,
                target,
                target_word,
            },
            ty,
            operator,
        }),
        boundary_before: true,
        registers: Vec::new(),
        locals: Default::default(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target],
        cross_access: reads,
        tiles: Vec::new(),
    });
    Ok(())
}
