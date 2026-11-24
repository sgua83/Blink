//! 静态值函数参与封闭集合；结合律与双侧单位元由算法定义，不做运行时猜测。
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Reduce,
    Scan,
    SegmentedScan,
    SegmentedReduce,
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_custom_reduce"]
    fn source_custom_reduce(bytes:*const u8,length:usize)->u32;
}
impl Kind {
    pub fn from_source(name: &str) -> Option<Self> {
        match unsafe{source_custom_reduce(name.as_ptr(),name.len())}{
            1=>Some(Self::Reduce),2=>Some(Self::Scan),3=>Some(Self::SegmentedScan),4=>Some(Self::SegmentedReduce),_=>None,
        }
    }
}

#[repr(u32)]enum ValueKernelError {Valid,Missing,Reference}
#[repr(C)]struct ValueKernelSource {error:ValueKernelError,explicit:u32,reference:u64,name:crate::syntax::wire::Text,captures:crate::syntax::wire::Slice,types:crate::syntax::wire::Slice}
#[repr(C)]#[derive(Clone,Copy)]struct ValueKernelCursor {phase:u32,index:u64}
#[repr(C)]#[derive(Clone,Copy)]struct ValueKernelFacts {unique:u32,valid:u32,parameters:u64,captures:u64,explicit:u32,uniform:u32,views:u32,reads:u32,writes:u32}
#[repr(u32)]enum ValueKernelNeed {Resolve,Unknown,Signature,InvalidSignature,Begin,Parameter,BeginCaptures,Capture,InvalidCapture,PushCapture,EndCaptures,Explicit,Specialize,Origin,Invoke,Ordinary,Value,Same,Publish}
#[repr(C)]struct ValueKernelStep {cursor:ValueKernelCursor,need:ValueKernelNeed,index:u64}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::value_kernel_source"]fn value_kernel_source(source:crate::syntax::wire::SignatureNode)->ValueKernelSource;
 #[link_name="bootstrap.function_body::value_kernel_signature"]fn value_kernel_signature(declaration:crate::syntax::wire::SignatureNode,invocation:u32,parameters:u64,captures:u64)->u32;
 #[link_name="bootstrap.function_body::value_kernel_step"]fn value_kernel_step(cursor:ValueKernelCursor,facts:ValueKernelFacts)->ValueKernelStep;
}
async fn function(
    world: &World,
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    owner: &crate::generics::Instance,
    domain_path: &Arc<[DomainSite]>,
    expr: &Expr,
    ty: &Ty,
    parameters: &[&Ty],
    functions: &mut native_function::Module,
) -> Result<ValueCode> {
    let d=&owner.definition;
    let source=unsafe {value_kernel_source(expr.signature_node())};
    let reference=expr.child::<crate::syntax::wire::ExprNode>(source.reference);
    match source.error {
        ValueKernelError::Valid=>{},
        ValueKernelError::Missing=>return d.source.fail(expr.span(),"B212","bind 需要静态值函数引用"),
        ValueKernelError::Reference=>return d.source.fail(reference.span(),"B212","集合需要静态函数引用，或 bind(静态函数引用, 统一值…)"),
    }
    let captures=expr.list::<crate::syntax::wire::ExprNode>(source.captures);
    let mut cursor=ValueKernelCursor {phase:0,index:0};
    let mut facts=ValueKernelFacts {unique:0,valid:0,parameters:parameters.len() as u64,captures:source.captures.count,explicit:source.explicit,uniform:0,views:0,reads:0,writes:0};
    let mut key=None;let mut builder=None;let mut inputs=Vec::new();let mut args=Vec::new();
    let mut explicit=None;let mut instance=None;let mut entry_origin=None;
    let mut returned=None;let mut ordinary=None;let mut value=None;
    loop {
        let step=unsafe {value_kernel_step(cursor,facts)};cursor=step.cursor;
        match step.need {
            ValueKernelNeed::Resolve=>{
                key=world.resolve(expr.source_view().text(source.name),&d.module,&d.imports,true).unique();
                facts.unique=u32::from(key.is_some());
            }
            ValueKernelNeed::Unknown=>return d.source.fail(expr.span(),"B158","未知或未导出的集合值函数"),
            ValueKernelNeed::Signature=>{
                let callee=&world.functions[key.as_ref().unwrap()];
                facts.valid=unsafe {value_kernel_signature(callee.function.signature_node(),u32::from(callee.execution==crate::execution::Requirement::Invocation),parameters.len() as u64,captures.len() as u64)};
            }
            ValueKernelNeed::InvalidSignature=>return d.source.fail(expr.span(),"B212","集合需要 compute 值函数：投影接收一个源值，单位元无元素参数，合并器接收两个结果值；bind 参数追加在元素参数之后"),
            ValueKernelNeed::Begin=>{
                let mut b=unsafe {world.call(Builder::new(world,owner,program,0,Some(&owner.name))).await}?;
                b.source_context=Arc::new(SourceContext {domain_path:domain_path.clone(),..(*b.source_context).clone()});
                b.functions=std::mem::take(functions);b.roots(env);builder=Some(b);
            }
            ValueKernelNeed::Parameter=>{
                let b=builder.as_mut().unwrap();let ty=parameters[step.index as usize];
                let regs:Vec<_>=ty.components.iter().map(|(s,_)|b.reg(*s)).collect();inputs.extend(&regs);
                args.push(cooperative::Argument::Value(Value {proof:bounds::Proof::default(),literal:false,ty:ty.clone(),regs,uniform:false,constant:None}));
            }
            ValueKernelNeed::BeginCaptures=>builder.as_mut().unwrap().in_function=true,
            ValueKernelNeed::Capture=>{
                let capture=captures.at(step.index as usize);let b=builder.as_mut().unwrap();
                let result=unsafe {world.call(b.expr(&capture)).await}?;let captured=b.value_result(result,capture.span())?;
                facts.uniform=u32::from(captured.uniform);facts.views=u32::from(captured.ty.contains_views());
                facts.reads=u32::from(!b.reads.is_empty());facts.writes=u32::from(!b.writes.is_empty());value=Some(captured);
            }
            ValueKernelNeed::InvalidCapture=>return d.source.fail(captures.at(step.index as usize).span(),"B213","bind 只能捕获执行统一的普通值，不能捕获资源或读取数据集元素"),
            ValueKernelNeed::PushCapture=>args.push(cooperative::Argument::Value(value.take().unwrap())),
            ValueKernelNeed::EndCaptures=>builder.as_mut().unwrap().in_function=false,
            ValueKernelNeed::Explicit=>{
                let arguments=expr.list(source.types);
                explicit=Some(unsafe {world.call(owner.bindings.arguments(world,&arguments,(&d.source,&d.module,&d.imports),reference.span())).await}?);
            }
            ValueKernelNeed::Specialize=>{
                instance=Some(unsafe {world.call(builder.as_ref().unwrap().specialized_arguments(key.take().unwrap(),&args,explicit.as_deref(),reference.span())).await}?);explicit=None;
            }
            ValueKernelNeed::Origin=>{
                let b=builder.as_ref().unwrap();let instance=instance.as_ref().unwrap();
                let source_site=|span|b.source_definition.as_ref().map(|origin|Arc::new(SourceSite {origin:origin.clone(),context:b.source_context.clone(),span,syntax:SourceSyntax::Expression}));
                entry_origin=Some(Arc::new(ValueCodeEntry {definition:instance.definition.source_origin().map(Arc::new),instance:Arc::from(instance.name.as_str()),reference:source_site(reference.span()),capture_parameter_start:parameters.len(),captures:captures.iter().map(|capture|source_site(capture.span())).collect()}));
            }
            ValueKernelNeed::Invoke=>returned=unsafe {world.call(builder.as_mut().unwrap().invoke_returned(instance.take().unwrap(),std::mem::take(&mut args),reference.span(),None,None)).await}?,
            ValueKernelNeed::Ordinary=>ordinary=builder.as_ref().unwrap().ordinary_return(returned.take(),expr.span())?,
            ValueKernelNeed::Value=>value=Some(builder.as_ref().unwrap().value_result(ordinary.take(),expr.span())?),
            ValueKernelNeed::Same=>builder.as_ref().unwrap().require_same(ty,&value.as_ref().unwrap().ty,expr.span())?,
            ValueKernelNeed::Publish=>{
                let mut b=builder.take().unwrap();*functions=std::mem::take(&mut b.functions);
                return Ok(ValueCode {entry_origin,generated_origin:None,registers:b.registers,locals:b.locals,code:b.code,inputs,outputs:value.take().unwrap().regs});
            }
        }
    }
}

pub(super) async fn compile(
    world: &World,
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &Expressions,
    owner: &crate::generics::Instance,
    span: Span,
    facts: &mut prefix::Facts,
    map: bool,
    kind: Kind,
    domain_path: Arc<[DomainSite]>,
    native_functions: &mut native_function::Module,
    operation_source: Option<Arc<SourceSite>>,
) -> Result<()> {
    let d = &owner.definition;
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
        prefix::typed_field(program, env, &args.at(0), d, facts)?;
    let head_word = if segmented {
        let (heads, word, access, head_prefix) =
            prefix::field(program, env, &args.at(1), d, facts, Scalar::Bool)?;
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
        collective_value_field(program, env, &args.at(1 + offset), d)?;
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
            collective_field(program, env, &args.at(2 + offset), d, Scalar::U32)?;
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
        Some(unsafe { world.call(function(
            world,
            program,
            env,
            owner,
            &domain_path,
            &args.at(functions),
            &ty,
            &[&source_ty],
            native_functions,
        )).await }?)
    } else {
        None
    };
    let identity = unsafe { world.call(function(
        world,
        program,
        env,
        owner,
        &domain_path,
        &args.at(functions + usize::from(map)),
        &ty,
        &[],
        native_functions,
    )).await }?;
    let combine = unsafe { world.call(function(
        world,
        program,
        env,
        owner,
        &domain_path,
        &args.at(functions + 1 + usize::from(map)),
        &ty,
        &[&ty, &ty],
        native_functions,
    )).await }?;
    let writes: Vec<_> = std::iter::once(target)
        .chain(count.map(|c| c.binding))
        .collect();
    let mut write_regions = BTreeMap::new();
    for &(scalar, word) in &ty.components {
        let start = target_word + word; let end = start + scalar.size();
        write_regions.entry(target).or_insert_with(BTreeSet::new).insert((start, end));
        facts.written(program, target, start, end, None);
    }
    if let Some(count) = count {
        write_regions.entry(count.binding).or_insert_with(BTreeSet::new).insert((count.word, count.word + 4));
        facts.written(program, count.binding, count.word, count.word + 4, None);
    }
    let prefix_guarantees: Vec<_> = count.or(prefix.filter(|_| scan)).map(|count| PrefixRelation { domain_region: None, domain: target, count }).into_iter().collect();
    for &relation in &prefix_guarantees { facts.grant(relation); }
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
        operation_source,
        generated_origin: None,
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
        write_regions,
        prefix_guarantees,
        element_guarantees: Vec::new(),
        cross_access: reads,
        tiles: Vec::new(),
    });
    Ok(())
}
