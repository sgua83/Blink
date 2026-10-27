//! 每个段尾拥有一个唯一输出位置；计数由同一聚合阶段生产有效域。
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
    if args.len() != 4 {
        return d.source.fail(
            span,
            "B169",
            "分段归约需要源数值、同源 bool 段头、目标数值和 u32 计数字段",
        );
    }
    let (source, source_word, source_access, prefix, ty) =
        prefix::typed_field(program, env, &args[0], d, facts)?;
    let (heads, head_word, head_access, head_prefix) =
        prefix::field(program, env, &args[1], d, facts, Scalar::Bool)?;
    let (target, target_word, target_access, target_ty) =
        collective_value_field(program, env, &args[2], d)?;
    let (count, count_word, count_access) =
        collective_field(program, env, &args[3], d, Scalar::U32)?;
    if !ty.is_numeric() || !operator.accepts(ty.components[0].0) || ty != target_ty {
        return d.source.fail(
            span,
            "B175",
            "分段归约需要同类型数值标量／向量；按位算子只接受整数",
        );
    }
    if !matches!(source_access, Access::Read | Access::InOut)
        || heads != source
        || !matches!(head_access, Access::Read | Access::InOut)
        || head_prefix.is_some_and(|p| Some(p) != prefix)
        || !target_access.writes()
        || !count_access.writes()
        || prefix::fields_overlap(source, source_word, &ty, target, target_word, &target_ty)
        || prefix::fields_overlap(
            source,
            head_word,
            &Ty::scalar(Scalar::Bool),
            target,
            target_word,
            &target_ty,
        )
        || source == count
        || target == count
        || prefix.is_some_and(|p| p.binding == target || p.binding == count)
    {
        return d.source.fail(
            span,
            "B170",
            "分段归约需要同域可读数值与段头、非重叠可写目标及独立输出计数",
        );
    }
    if program.buffers[count].count != Some(1) || program.buffers[count].ty.components.len() != 1 {
        return d.source.fail(
            args[3].span,
            "B171",
            "分段归约计数必须为形状 1、单 u32 字段",
        );
    }
    if let (Some(a), Some(b)) = (program.buffers[source].count, program.buffers[target].count)
        && b < a
    {
        return d.source.fail(
            span,
            "B188",
            "分段归约目标容量必须容纳最坏情况下的全部输入段",
        );
    }
    let count = PrefixCount {
        binding: count,
        word: count_word,
    };
    facts.written(&[target, count.binding]);
    facts.grant(PrefixRelation {
        domain: target,
        count,
    });
    let reads: Vec<_> = std::iter::once(source)
        .chain(prefix.map(|p| p.binding))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    program.stages.push(Stage {
        domain: source,
        prefix,
        selection: None,
        operation: Operation::SegmentedReduce(SegmentedReduction {
            values: ReductionSpec {
                fields: Reduction {
                    source,
                    source_word,
                    target,
                    target_word,
                },
                ty,
                operator,
            },
            head_word,
            count,
        }),
        boundary_before: true,
        registers: Vec::new(),
        locals: Vec::new(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target, count.binding],
        cross_access: reads,
        tiles: Vec::new(),
    });
    Ok(())
}
