//! 稳定排序按不可变键建立唯一目标序号；源记录与容量尾部保持原值。
use super::*;
pub(super) fn compile(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &[Expr],
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
) -> Result<()> {
    if args.len() != 2 {
        return d.source.fail(
            span,
            "B169",
            "stable_sort_by_key 需要 u32/i32/f32 标量源键字段和输出 dataset",
        );
    }
    let (source, key_word, access, prefix, ty) =
        prefix::typed_field(program, env, &args[0], d, facts)?;
    let key = if ty == Ty::scalar(Scalar::U32) {
        SortKey::U32
    } else if ty == Ty::scalar(Scalar::I32) {
        SortKey::I32
    } else if ty == Ty::scalar(Scalar::F32) {
        SortKey::F32
    } else {
        return d.source.fail(
            args[0].span,
            "B175",
            "排序键需要 u32/i32/f32 标量字段；向量、bool、记录和资源不能作为键",
        );
    };
    let (target, write) = compact::data(env, &args[1], d)?;
    if access != Access::Read
        || !write.writes()
        || source == target
        || prefix.is_some_and(|p| p.binding == target)
    {
        return d.source.fail(
            span,
            "B170",
            "排序需要独立 read 输入和可写输出，输出不得改写输入域计数",
        );
    }
    if program.buffers[source].ty != program.buffers[target].ty {
        return d
            .source
            .fail(args[1].span, "B189", "排序输入／输出的完整记录类型必须相同");
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
            key_word,
            key,
            target,
        }),
        boundary_before: true,
        registers: Vec::new(),
        locals: Vec::new(),
        code: Vec::new(),
        reads: reads.clone(),
        writes: vec![target],
        cross_access: reads,
    });
    Ok(())
}
