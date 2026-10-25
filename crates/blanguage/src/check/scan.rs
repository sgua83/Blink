//! 扫描持有输入的有效域，数值分量与分段计数沿用各自的结合元。
use super::*;
pub(super) fn compile(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    operator: CollectiveOperator,
    segmented: bool,
    args: &[Expr],
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
) -> Result<()> {
    if args.len() != if segmented { 3 } else { 2 } {
        return d.source.fail(
            span,
            "B169",
            "扫描需要源字段和输出字段；分段扫描还需要源记录的 bool 段起点字段",
        );
    }
    let (source, source_word, source_access, prefix, ty) =
        prefix::typed_field(program, env, &args[0], d, facts)?;
    let (target, target_word, target_access, target_ty) =
        collective_value_field(program, env, args.last().unwrap(), d)?;
    if !ty.is_numeric() || !operator.accepts(ty.components[0].0) || ty != target_ty {
        return d.source.fail(
            span,
            "B175",
            "扫描需要同类型数值标量／向量字段；按位算子只接受整数",
        );
    }
    let head_word = if segmented {
        let (heads, word, access, head_prefix) =
            prefix::field(program, env, &args[1], d, facts, Scalar::Bool)?;
        if heads != source
            || access != Access::Read
            || head_prefix.is_some_and(|p| Some(p) != prefix)
        {
            return d.source.fail(
                span,
                "B170",
                "分段起点必须来自同一 read 输入记录，和源值共享元素域",
            );
        }
        Some(word)
    } else {
        None
    };
    if source == target
        || source_access != Access::Read
        || !target_access.writes()
        || prefix.is_some_and(|p| p.binding == target)
    {
        return d.source.fail(
            span,
            "B170",
            "扫描需要独立 read 输入和可写输出，输出不得改写输入域计数",
        );
    }
    if let (Some(a), Some(b)) = (program.buffers[source].count, program.buffers[target].count) {
        if (prefix.is_none() && a != b) || (prefix.is_some() && b < a) {
            return d
                .source
                .fail(span, "B186", "扫描输出形状必须满足输入域容量");
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
        domain: source,
        prefix,
        selection: None,
        operation: Operation::ScanExclusive(Scan {
            values: Reduction {
                source,
                source_word,
                target,
                target_word,
            },
            ty,
            operator,
            head_word,
        }),
        boundary_before: true,
        registers: Vec::new(),
        locals: Vec::new(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target],
        cross_access: reads,
        tiles: Vec::new(),
    });
    Ok(())
}
