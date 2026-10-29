//! 静态值函数参与封闭集合；结合律与双侧单位元由算法定义，不做运行时猜测。
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Reduce,
    Scan,
    SegmentedScan,
    SegmentedReduce,
}
impl Kind {
    pub fn from_source(name: &str) -> Option<Self> {
        Some(match name.strip_suffix("_map").unwrap_or(name) {
            "reduce" => Self::Reduce,
            "scan_exclusive" => Self::Scan,
            "scan_segmented_exclusive" => Self::SegmentedScan,
            "reduce_segmented" => Self::SegmentedReduce,
            _ => return None,
        })
    }
}
fn function(
    world: &World,
    program: &Program,
    env: &BTreeMap<String, Root>,
    d: &Arc<Definition>,
    expr: &Expr,
    ty: &Ty,
    parameters: &[&Ty],
) -> Result<ValueCode> {
    let (reference, captures) = match &expr.kind {
        ExprKind::Call(name, args) if name == "bind" => {
            let Some(reference) = args.first() else {
                return d.source.fail(expr.span, "B212", "bind 需要静态值函数引用");
            };
            (reference, &args[1..])
        }
        _ => (expr, &[][..]),
    };
    let (name, explicit) = match &reference.kind {
        ExprKind::Name(name) => (name, None),
        ExprKind::FunctionRef(name, arguments) => (name, Some(arguments.as_slice())),
        _ => {
            return d.source.fail(
                reference.span,
                "B212",
                "集合需要静态函数引用，或 bind(静态函数引用, 统一值…)",
            );
        }
    };
    let Some(key) = world.resolve(name, &d.module, &d.imports, true) else {
        return d.source.fail(expr.span, "B158", "未知或未导出的集合值函数");
    };
    let callee = &world.functions[&key];
    if callee.execution != crate::execution::Requirement::Invocation
        || callee.function.kind != FunctionKind::Value
        || callee.function.target != Target::Compute
        || callee.function.params.len() != parameters.len() + captures.len()
        || callee
            .function
            .params
            .iter()
            .any(|p| p.access != Access::Value || p.ty.dataset || p.ty.tile || p.ty.view)
    {
        return d.source.fail(
            expr.span,
            "B212",
            "集合需要 compute 值函数：投影接收一个源值，单位元无元素参数，合并器接收两个结果值；bind 参数追加在元素参数之后",
        );
    }
    let mut builder = Builder::new(world, d, program, 0);
    builder.roots(env);
    let mut inputs = Vec::new();
    let mut args = Vec::new();
    for ty in parameters {
        let regs: Vec<_> = ty.components.iter().map(|(s, _)| builder.reg(*s)).collect();
        inputs.extend(&regs);
        args.push(cooperative::Argument::Value(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: (*ty).clone(),
            regs,
            uniform: false,
            constant: None,
        }));
    }
    builder.in_function = true;
    for capture in captures {
        let result = builder.expr(capture)?;
        let value = builder.value_result(result, capture.span)?;
        if !value.uniform
            || value.ty.contains_views()
            || !builder.reads.is_empty()
            || !builder.writes.is_empty()
        {
            return d.source.fail(
                capture.span,
                "B213",
                "bind 只能捕获执行统一的普通值，不能捕获资源或读取数据集元素",
            );
        }
        args.push(cooperative::Argument::Value(value));
    }
    builder.in_function = false;
    let instance = builder.specialized_arguments(key, &args, explicit, reference.span)?;
    let result = builder.inline_instance(instance, args, reference.span)?;
    let result = builder.ordinary_return(result, expr.span)?;
    let result = builder.value_result(result, expr.span)?;
    builder.require_same(ty, &result.ty, expr.span)?;
    Ok(ValueCode {
        registers: builder.registers,
        locals: builder.locals,
        code: builder.code,
        inputs,
        outputs: result.regs,
    })
}
pub(super) fn compile(
    world: &World,
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &[Expr],
    d: &Arc<Definition>,
    span: Span,
    facts: &mut prefix::Facts,
    map: bool,
    kind: Kind,
) -> Result<()> {
    let scan = matches!(kind, Kind::Scan | Kind::SegmentedScan);
    let segmented = matches!(kind, Kind::SegmentedScan | Kind::SegmentedReduce);
    let counted = kind == Kind::SegmentedReduce;
    let offset = usize::from(segmented);
    let functions = 2 + offset + usize::from(counted);
    if args.len() != functions + 2 + usize::from(map) {
        return d.source.fail(
            span,
            "B169",
            "自定义集合需要源字段、目标字段、单位元和合并函数；分段形式在源值后提供同源 bool 段头，分段归约在目标后提供输出计数，映射形式在单位元前提供投影函数",
        );
    }
    let (source, source_word, read, prefix, source_ty) =
        prefix::typed_field(program, env, &args[0], d, facts)?;
    let head_word = if segmented {
        let (heads, word, access, head_prefix) =
            prefix::field(program, env, &args[1], d, facts, Scalar::Bool)?;
        if heads != source
            || !matches!(access, Access::Read | Access::InOut)
            || head_prefix.is_some_and(|p| Some(p) != prefix)
        {
            return d.source.fail(
                span,
                "B170",
                "分段起点必须来自同一可读输入记录，和源值共享元素域",
            );
        }
        Some(word)
    } else {
        None
    };
    let (target, target_word, write, ty) =
        collective_value_field(program, env, &args[1 + offset], d)?;
    if !map && source_ty != ty || ty.contains_views() || source_ty.contains_views() {
        return d.source.fail(
            span,
            "B175",
            "无投影的集合需要相同普通值类型，聚合值不能持有资源引用",
        );
    }
    if prefix::fields_overlap(source, source_word, &source_ty, target, target_word, &ty)
        || head_word.is_some_and(|word| {
            prefix::fields_overlap(
                source,
                word,
                &Ty::scalar(Scalar::Bool),
                target,
                target_word,
                &ty,
            )
        })
        || !matches!(read, Access::Read | Access::InOut)
        || !write.writes()
        || prefix.is_some_and(|p| p.binding == target)
    {
        return d.source.fail(
            span,
            "B170",
            "集合需要可读输入和可写输出，目标不能覆盖源值、段头或输入域计数",
        );
    }
    let count = if counted {
        let (binding, word, access) =
            collective_field(program, env, &args[2 + offset], d, Scalar::U32)?;
        if !access.writes()
            || binding == source
            || binding == target
            || prefix.is_some_and(|p| p.binding == binding)
        {
            return d.source.fail(
                span,
                "B170",
                "分段归约需要独立的可写目标与输出计数，不能改写输入域计数",
            );
        }
        if program.buffers[binding].count != Some(1)
        {
            return d
                .source
                .fail(span, "B171", "分段归约计数必须为形状 1 的 u32 计数字段");
        }
        Some(PrefixCount { binding, word })
    } else {
        None
    };
    if scan || counted {
        if let (Some(n), Some(capacity)) =
            (program.buffers[source].count, program.buffers[target].count)
            && capacity < n
        {
            return d
                .source
                .fail(span, "B188", "扫描／分段归约输出容量必须容纳全部源元素");
        }
    } else if program.buffers[target].count != Some(1) {
        return d.source.fail(span, "B171", "归约输出形状必须为 1");
    }
    let project = if map {
        Some(function(
            world,
            program,
            env,
            d,
            &args[functions],
            &ty,
            &[&source_ty],
        )?)
    } else {
        None
    };
    let identity = function(
        world,
        program,
        env,
        d,
        &args[functions + usize::from(map)],
        &ty,
        &[],
    )?;
    let combine = function(
        world,
        program,
        env,
        d,
        &args[functions + 1 + usize::from(map)],
        &ty,
        &[&ty, &ty],
    )?;
    let writes: Vec<_> = std::iter::once(target)
        .chain(count.map(|c| c.binding))
        .collect();
    facts.written(&writes);
    if let Some(count) = count.or(prefix.filter(|_| scan)) {
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
    let values = CustomReduction {
        fields: Reduction {
            source,
            source_word,
            target,
            target_word,
        },
        ty,
        source_ty,
        project,
        identity,
        combine,
    };
    program.stages.push(Stage {
        domain: source,
        prefix,
        selection: None,
        operation: if let Some(count) = count {
            Operation::CustomSegmentedReduce(CustomSegmentedReduction {
                values,
                head_word: head_word.unwrap(),
                count,
            })
        } else if scan {
            Operation::CustomScan(CustomScan { values, head_word })
        } else {
            Operation::CustomReduce(values)
        },
        boundary_before: true,
        registers: Vec::new(),
        locals: Default::default(),
        code: Vec::new(),
        reads: reads.clone(),
        writes,
        cross_access: reads,
        tiles: Vec::new(),
    });
    Ok(())
}
