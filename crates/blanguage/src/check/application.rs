//! 应用入口是一次普通 CPU 值函数调用，不伪造 dataset 或 parallel 域。
use super::*;
pub(crate) fn compile_application(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<crate::application::Application> {
    let d = &instance.definition;
    validate_signature(d)?;
    let result = if d.function.result.name == "void" {
        None
    } else {
        Some(world.ty(
            &d.function.result,
            &d.source,
            d.function.span,
            &d.module,
            &d.imports,
        )?)
    };
    if d.function.kind != FunctionKind::Value
        || d.function.target != Target::Cpu
        || !d.function.params.is_empty()
        || !result.is_some_and(|result| {
            [Ty::scalar(Scalar::U32), Ty::scalar(Scalar::I32)].contains(&result)
        })
    {
        return d.source.fail(
            d.function.span,
            "B262",
            "应用入口需要无参数的 u32／i32 普通函数，显式 on cpu；返回值是进程退出码",
        );
    }
    let context = Program {
        name: instance.name.clone(),
        target: Target::Cpu,
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        uniforms: Vec::new(),
        uniform_bytes: 0,
        source_stages: 0,
        stages: Vec::new(),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(),
    };
    let mut builder = Builder::new(world, d, &context, 0);
    builder.in_function = true;
    builder.calls.push(instance.name.clone());
    let requirements = contracts::resolve(&builder, d, &builder.env)?;
    contracts::assume(&mut builder.env, &requirements);
    let result = builder
        .function_body(d.function.body.as_ref().unwrap())?
        .unwrap();
    let mut body = ValueCode {
        registers: builder.registers,
        locals: builder.locals,
        code: builder.code,
        inputs: Vec::new(),
        outputs: result.regs,
    };
    crate::optimize::value_code(&mut body);
    Ok(crate::application::Application {
        name: instance.name.clone(),
        body,
    })
}
impl Builder<'_> {
    pub(super) fn println(&mut self, args: &[Expr], span: Span) -> Result<Value> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B260", "println 是 CPU 标准输出操作，要求显式 on cpu");
        }
        if args.len() != 1 {
            return self.fail(span, "B261", "println 需要一个 u32、i32 或 bool 标量");
        }
        let value = self.expr(&args[0])?;
        if ![
            Ty::scalar(Scalar::U32),
            Ty::scalar(Scalar::I32),
            Ty::scalar(Scalar::Bool),
        ]
        .contains(&value.ty)
        {
            return self.fail(
                args[0].span,
                "B261",
                "println 当前接受 u32、i32 或 bool 标量",
            );
        }
        let dst = self.reg(Scalar::Bool);
        self.code.push(Inst::Print {
            dst,
            value: value.regs[0],
        });
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::Bool),
            regs: vec![dst],
            uniform: false,
            constant: None,
        })
    }
}
