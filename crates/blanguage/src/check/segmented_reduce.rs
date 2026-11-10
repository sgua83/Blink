//! 每个段尾拥有一个唯一输出位置；计数由同一聚合阶段生产有效域。
use super::*;
pub(super) fn compile(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    operator: CollectiveOperator,
    args: &Expressions,
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
    operation_source: Option<Arc<SourceSite>>,
) -> Result<()> {
    if args.len() != 4 {
        return d.source.fail(
            span,
            "B169",
            "分段归约需要源数值、同源 bool 段头、目标数值和 u32 计数字段",
        );
    }
    let (source, source_word, source_access, prefix, ty) =
        prefix::typed_field(program, env, &args.at(0), d, facts)?;
    let (heads, head_word, head_access, head_prefix) =
        prefix::field(program, env, &args.at(1), d, facts, Scalar::Bool)?;
    let (target, target_word, target_access, target_ty) =
        collective_value_field(program, env, &args.at(2), d)?;
    let (count, count_word, count_access) =
        collective_field(program, env, &args.at(3), d, Scalar::U32)?;
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
    if program.buffers[count].count != Some(1) {
        return d.source.fail(
            args.at(3).span(),
            "B171",
            "分段归约计数必须为形状 1 的 u32 计数字段",
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
    let mut write_regions = BTreeMap::new();
    for &(scalar, word) in &ty.components {
        let start = target_word + word; let end = start + scalar.size();
        write_regions.entry(target).or_insert_with(BTreeSet::new).insert((start, end));
        facts.written(program, target, start, end, None);
    }
    write_regions.entry(count.binding).or_insert_with(BTreeSet::new).insert((count.word, count.word + 4));
    facts.written(program, count.binding, count.word, count.word + 4, None);
    let prefix_guarantees = vec![PrefixRelation { domain_region: None,
        domain: target,
        count,
    }];
    for &relation in &prefix_guarantees { facts.grant(relation); }
    let reads: Vec<_> = std::iter::once(source)
        .chain(prefix.map(|p| p.binding))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    program.stages.push(Stage {
        operation_source,
        generated_origin: None,
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
        locals: Default::default(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target, count.binding],
        write_regions,
        prefix_guarantees,
        element_guarantees: Vec::new(),
        cross_access: reads,
        tiles: Vec::new(),
    });
    Ok(())
}
