//! 稳定压缩筛选拥有唯一散写证明：布尔谓词的独占前缀决定输出序号。
use super::*;
pub(super) fn data(
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
) -> Result<(usize, Access)> {
    if let ExprKind::Name(name) = &expr.kind {
        if let Some(Root::Data(binding, access)) = env.get(name) {
            return Ok((*binding, *access));
        }
    }
    d.source
        .fail(expr.span, "B190", "集合输入／输出需要命名 dataset")
}
pub(super) fn compile(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &[Expr],
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
) -> Result<()> {
    if args.len() != 4 {
        return d.source.fail(
            span,
            "B169",
            "compact 需要输入、bool 谓词字段、输出和 u32 单值计数字段",
        );
    }
    let (input, prefix) = prefix::input(program, env, &args[0], d, facts)?;
    let (source, read) = data(env, &input, d)?;
    let (flags, flag_word, flags_read) = collective_field(program, env, &args[1], d, Scalar::Bool)?;
    let (target, write) = data(env, &args[2], d)?;
    let (count, count_word, count_write) =
        collective_field(program, env, &args[3], d, Scalar::U32)?;
    if read != Access::Read
        || flags_read != Access::Read
        || !write.writes()
        || !count_write.writes()
        || target == count
        || prefix.is_some_and(|p| p.binding == target || p.binding == count)
        || [source, flags].iter().any(|&b| b == target || b == count)
    {
        return d.source.fail(
            span,
            "B170",
            "compact 需要不可变输入／谓词，输出与计数不得与读资源或彼此别名",
        );
    }
    if program.buffers[source].ty != program.buffers[target].ty {
        return d
            .source
            .fail(args[2].span, "B189", "compact 输入／输出的记录类型必须相同");
    }
    if program.buffers[count].count != Some(1) || program.buffers[count].ty.components.len() != 1 {
        return d.source.fail(
            args[3].span,
            "B171",
            "compact 计数输出必须为形状 1、单 u32 字段",
        );
    }
    if let (Some(a), Some(b)) = (program.buffers[source].count, program.buffers[flags].count) {
        if a != b {
            return d
                .source
                .fail(span, "B187", "compact 的输入与谓词形状必须相同");
        }
    }
    if let (Some(a), Some(b)) = (program.buffers[source].count, program.buffers[target].count) {
        if b < a {
            return d
                .source
                .fail(span, "B188", "compact 输出容量必须容纳全部输入");
        }
    }
    facts.written(&[target, count]);
    facts.grant(PrefixRelation {
        domain: target,
        count: PrefixCount {
            binding: count,
            word: count_word,
        },
    });
    let reads: Vec<_> = [source, flags]
        .into_iter()
        .chain(prefix.map(|p| p.binding))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    program.stages.push(Stage {
        tiles: Vec::new(),
        domain: source,
        prefix,
        operation: Operation::Compact(Compaction {
            source,
            flags,
            flag_word,
            target,
            count,
            count_word,
        }),
        boundary_before: true,
        registers: Vec::new(),
        locals: Vec::new(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target, count],
        cross_access: reads,
    });
    Ok(())
}
