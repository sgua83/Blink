//! 输出与计数在同一阶段配对；接纳原语独占预留及完整记录写入。
use super::*;

pub(super) fn clear(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &[Expr],
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
) -> Result<()> {
    if args.len() != 2 {
        return d
            .source
            .fail(span, "B220", "clear_prefix(输出, 计数字段) 需要两个参数");
    }
    let (target, access) = compact::data(env, &args[0], d)?;
    let (count, word, count_access) = collective_field(program, env, &args[1], d, Scalar::U32)?;
    if !matches!(access, Access::Write | Access::InOut)
        || !matches!(count_access, Access::Write | Access::InOut)
        || target == count
        || program.buffers[count].count != Some(1)
        || program.buffers[count].ty.components.len() != 1
    {
        return d.source.fail(
            span,
            "B221",
            "clear_prefix 需要独立的普通可写输出和形状 1、单 u32 计数",
        );
    }
    facts.written(&[count]);
    facts.grant(PrefixRelation {
        domain: target,
        count: PrefixCount {
            binding: count,
            word,
        },
    });
    program.stages.push(Stage {
        domain: count,
        prefix: None,
        selection: None,
        operation: Operation::Map,
        boundary_before: true,
        registers: vec![Scalar::U32],
        locals: Vec::new(),
        code: vec![
            Inst::Constant(0, Scalar::U32, 0),
            Inst::Store {
                binding: count,
                index: 0,
                word,
                value: 0,
            },
        ],
        reads: Vec::new(),
        writes: vec![count],
        cross_access: Vec::new(),
        tiles: Vec::new(),
    });
    Ok(())
}

impl Builder<'_> {
    pub(super) fn append(&mut self, args: &[Expr], span: Span) -> Result<Value> {
        if args.len() != 3 {
            return self.fail(
                span,
                "B220",
                "append(输出, 计数字段, 完整记录值) 需要三个参数",
            );
        }
        let ExprKind::Name(name) = &args[0].kind else {
            return self.fail(args[0].span, "B221", "append 输出需要命名 dataset");
        };
        let Some(Binding::Data(view)) = self.env.get(name).cloned() else {
            return self.fail(args[0].span, "B221", "append 输出需要命名 dataset");
        };
        let target = view.binding;
        let (binding, word, ty) = self.dataset_field(&args[1])?;
        let count = PrefixCount { binding, word };
        if !matches!(self.view_access[&target], Access::Write | Access::InOut)
            || self.view_access[&binding] != Access::InOut
            || target == binding
            || target == self.domain
            || binding == self.domain
            || self.program.buffers[binding].count != Some(1)
            || self.program.buffers[binding].ty.components.len() != 1
            || ty != Ty::scalar(Scalar::U32)
        {
            return self.fail(
                span,
                "B221",
                "append 需要域外普通可写输出及独立 inout 单 u32 计数；不能与并行域或彼此别名",
            );
        }
        let value = self.expr(&args[2])?;
        self.require_same(&self.program.buffers[target].ty, &value.ty, args[2].span)?;
        if self.appends.iter().any(|(output, previous)| {
            (*output == target || previous.binding == binding)
                && (*output != target || *previous != count)
        }) {
            return self.fail(
                span,
                "B222",
                "同一并行阶段的 append 输出与计数必须一对一配对",
            );
        }
        self.appends.insert(target, count);
        self.reads.insert(binding);
        self.writes.extend([target, binding]);
        self.cross_access.extend([target, binding]);
        let accepted = self.reg(Scalar::Bool);
        self.code.push(Inst::Append(Append {
            accepted,
            target,
            count,
            values: value
                .ty
                .components
                .iter()
                .zip(&value.regs)
                .map(|((_, byte), r)| (byte / 4, *r))
                .collect(),
        }));
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::Bool),
            regs: vec![accepted],
            uniform: false,
            constant: None,
        })
    }
}
