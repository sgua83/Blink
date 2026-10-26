use crate::{
    Definition, World,
    ast::*,
    ir::{self, *},
};
mod append;
mod array;
mod atomic;
mod bounds;
mod compact;
mod constant;
use constant::constant_value;
mod application;
mod contracts;
mod cooperative;
mod custom_reduce;
mod effects;
mod external;
mod flow;
pub(crate) mod generics;
mod matrix;
mod ownership;
mod prefix;
mod records;
mod reduce;
mod returns;
mod scan;
mod segmented_reduce;
mod selection;
mod sort;
mod subgroup;
pub(crate) use application::compile_application;
mod texture;
mod tile;
mod view;
use flow::{Flow, LoopContext};
use ownership::Position;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
enum Root {
    Data(usize, Access),
    Acceleration(usize),
    Texture(usize),
    Uniform(Ty, u32),
    Literal(Ty, Vec<u32>),
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ValueIdentity {
    Register(Reg),
    // 资源的长度身份不随每次装载而变化；只存在于编译期。
    DataLength(usize),
    TileLength,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum IndexDomain {
    Data(usize),
    Fixed(u32),
    Tile,
    DataLength(usize),
    TileLength,
    View([ValueIdentity; 3]),
    ViewLength([ValueIdentity; 3]),
    ApplicationData(Reg),
    ApplicationLength(Reg),
}
type Domains = BTreeSet<IndexDomain>;
#[derive(Clone)]
struct Value {
    proof: bounds::Proof,
    // 尚未由绑定、参数、返回值或显式构造固定类型的数值字面量表达式。
    literal: bool,
    ty: Ty,
    regs: Vec<Reg>,
    uniform: bool,
    constant: Option<Vec<u32>>,
}
#[derive(Clone)]
enum Binding {
    ApplicationData(application::Data),
    AtomicBorrow(Place),
    Owned(OwnedBinding),
    Value(Value),
    Data(DatasetView),
    Tile(TileView),
    TileElement {
        view: TileView,
        index: Reg,
    },
    ViewElement {
        view: Value,
        index: Reg,
    },
    Acceleration(usize),
    Texture(texture::TextureReference),
    Element {
        binding: usize,
        index: Reg,
        writable: bool,
    },
}
#[derive(Clone)]
struct OwnedBinding {
    place: Place,
    proof: bounds::Proof,
    uniform: bool,
    initialization: Option<Vec<Position>>,
}
#[derive(Clone)]
struct DatasetView {
    binding: usize,
    length: bounds::Proof,
}
#[derive(Clone)]
struct TileView {
    tile: usize,
    ty: Ty,
    words: Vec<u32>,
    length: bounds::Proof,
}
enum CursorSource {
    ApplicationData(application::Data),
    Data(usize),
    Tile(TileView),
    View(Value),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TileWidth {
    Fixed(u32),
    Parameter,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TileOrigin {
    Bound(TileSource),
    Parameter,
}
struct CheckedTile {
    source: TileOrigin,
    ty: Ty,
    width: TileWidth,
    access: Access,
}
#[derive(Clone, PartialEq, Eq)]
enum Place {
    ApplicationElement {
        ty: Ty,
        handle: Reg,
        index: Reg,
        index_range: (u32, u32),
        stride: u32,
        words: Vec<u32>,
        writable: bool,
    },
    // 独立函数检查的符号位置；program 内联直接借用调用方的实际 Place。
    AtomicParameter(Ty),
    ViewElement {
        ty: Ty,
        address: [Reg; 2],
        index: Reg,
        stride: u32,
        words: Vec<u32>,
    },
    TileElement {
        ty: Ty,
        tile: usize,
        index: Reg,
        words: Vec<u32>,
    },
    Local(Ty, Vec<usize>, std::ops::Range<usize>),
    ReadOnlyLocal(Ty, Vec<usize>, std::ops::Range<usize>),
    Indexed {
        ty: Ty,
        base: Box<Place>,
        offset: Reg,
        words: Vec<u32>,
    },
    Element {
        ty: Ty,
        binding: usize,
        index: Reg,
        words: Vec<u32>,
        writable: bool,
    },
}

pub(crate) fn compile_program(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<Program> {
    let name = &instance.name;
    let d = &instance.definition;
    let mut buffers = Vec::new();
    let mut accelerations = Vec::new();
    let mut textures = Vec::new();
    let mut uniforms = Vec::new();
    let mut env = BTreeMap::new();
    let mut offset = 0;
    for p in &d.function.params {
        if p.ty.is_texture_resource() {
            if p.access != Access::Read {
                return d
                    .source
                    .fail(p.span, "B239", "texture2d 必须声明 read 资源权限");
            }
            env.insert(p.name.clone(), Root::Texture(textures.len()));
            textures.push(TextureBinding {
                name: p.name.clone(),
                count: texture::shape(world, &p.ty, d, p.span)?,
            });
            continue;
        }
        if p.ty.is_acceleration() {
            if p.access != Access::Read || d.function.target != Target::Gpu {
                return d.source.fail(
                    p.span,
                    "B182",
                    "acceleration 必须是 GPU program 的 read 资源",
                );
            }
            env.insert(p.name.clone(), Root::Acceleration(accelerations.len()));
            accelerations.push(p.name.clone());
            continue;
        }
        let ty = world.ty(&p.ty, &d.source, p.span, &d.module, &d.imports)?;
        if ty.contains_views() && (!p.ty.dataset || p.access != Access::Read) {
            return d.source.fail(
                p.span,
                "B185",
                "view 只能由 program 的 read dataset 持有，不能写出或作为 program 按值参数绑定",
            );
        }
        if p.ty.dataset {
            if p.access == Access::Value {
                return d.source.fail(
                    p.span,
                    "B101",
                    "dataset 必须声明 read、write、inout 或 atomic",
                );
            }
            let binding = buffers.len();
            let stride = ir::align_up(ty.size, ty.align);
            buffers.push(Buffer {
                name: p.name.clone(),
                access: p.access,
                ty,
                stride,
                count: world.shape(&p.ty.count, &d.source, p.span, &d.module, &d.imports)?,
            });
            env.insert(p.name.clone(), Root::Data(binding, p.access));
        } else {
            if p.access != Access::Value {
                return d
                    .source
                    .fail(p.span, "B102", "标量参数按值传递；访问权限只用于 dataset");
            }
            uniforms.push(Uniform {
                name: p.name.clone(),
                ty,
                offset: 0,
            });
        }
    }
    // word 0 为阶段长度，word 1 为设备分区起点，其后是各数据集长度。
    offset += 4 * (2 + buffers.len() as u32);
    for u in &mut uniforms {
        offset = ir::align_up(offset, u.ty.align);
        u.offset = offset;
        offset += u.ty.size;
        env.insert(u.name.clone(), Root::Uniform(u.ty.clone(), u.offset));
    }
    offset = ir::align_up(offset, 4);
    if offset > 128 {
        return d.source.fail(
            d.function.span,
            "B103",
            format!("程序参数占 {offset} 字节，当前设备 ABI 上限 128；大块参数应使用 read dataset"),
        );
    }
    let mut program = Program {
        name: name.clone(),
        target: d.function.target,
        buffers,
        accelerations,
        textures,
        uniforms,
        uniform_bytes: offset,
        source_stages: 0,
        stages: Vec::new(),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(),
    };
    program.prefix_requirements = prefix::relations(&program, &env, d, ContractKind::Requires)?;
    program.prefix_guarantees = prefix::relations(&program, &env, d, ContractKind::Ensures)?;
    let mut facts = prefix::Facts::new(&program.prefix_requirements);
    expand_program(
        world,
        d,
        &env,
        &mut program,
        &mut Vec::new(),
        &mut false,
        &mut facts,
    )?;
    program.source_stages = program.stages.len();
    crate::optimize::optimize(&mut program);
    Ok(program)
}
fn expand_program(
    world: &World,
    d: &Arc<Definition>,
    env: &BTreeMap<String, Root>,
    program: &mut Program,
    stack: &mut Vec<String>,
    boundary: &mut bool,
    facts: &mut prefix::Facts,
) -> Result<()> {
    let key = format!("{}::{}", d.module, d.function.name);
    if stack.contains(&key) {
        return d
            .source
            .fail(d.function.span, "B105", "计算程序递归调用没有静态阶段边界");
    }
    stack.push(key);
    expand_stages(
        world,
        d,
        d.function.body.as_ref().unwrap(),
        env,
        program,
        stack,
        boundary,
        facts,
    )?;
    for contract in d
        .function
        .contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Ensures)
    {
        let relation = prefix::relation(program, env, d, contract)?;
        if !facts.holds(relation) {
            return d.source.fail(
                contract.span,
                "B193",
                "ensures prefix 未由本函数当前生产证明或有效输入能力支持",
            );
        }
    }
    stack.pop();
    Ok(())
}
fn expand_stages(
    world: &World,
    d: &Arc<Definition>,
    body: &[Stmt],
    env: &BTreeMap<String, Root>,
    program: &mut Program,
    stack: &mut Vec<String>,
    boundary: &mut bool,
    facts: &mut prefix::Facts,
) -> Result<()> {
    for statement in body {
        match &statement.kind {
            StmtKind::For(name, first, Some(last), body) => {
                if env.contains_key(name) {
                    return d.source.fail(
                        statement.span,
                        "B125",
                        format!("重复阶段 for 计数器或覆盖参数 {name}"),
                    );
                }
                let bound = |expr: &Expr| -> Result<u32> {
                    match stage_root(world, d, env, program, expr)? {
                        Root::Literal(ty, words) if ty == Ty::scalar(Scalar::U32) => Ok(words[0]),
                        _ => d.source.fail(
                            expr.span,
                            "B251",
                            "program 阶段 for 的范围必须是编译期 u32；动态循环放在 parallel 中",
                        ),
                    }
                };
                let first = bound(first)?;
                let last = bound(last)?;
                for index in first..last {
                    let mut child = env.clone();
                    child.insert(
                        name.clone(),
                        Root::Literal(Ty::scalar(Scalar::U32), vec![index]),
                    );
                    expand_stages(world, d, body, &child, program, stack, boundary, facts)?;
                }
            }
            StmtKind::Parallel(name, syntax, body) => {
                let data = &syntax.dataset;
                let Some(Root::Data(domain, domain_access)) = env.get(data) else {
                    return d
                        .source
                        .fail(statement.span, "B106", format!("{data} 不是数据集参数"));
                };
                let prefix = syntax
                    .count
                    .as_ref()
                    .map(|expr| prefix::resolve(program, env, *domain, expr, d, facts))
                    .transpose()?;
                let selection = syntax
                    .selection
                    .as_ref()
                    .map(|expr| {
                        let (binding, word, access) =
                            collective_field(program, env, expr, d, Scalar::Bool)?;
                        if access != Access::Read
                            && !(binding == *domain && access == Access::InOut)
                        {
                            return d.source.fail(
                                expr.span,
                                "B170",
                                "where 谓词必须是只读 bool 字段，或当前 inout 独占域的 bool 字段",
                            );
                        }
                        if let (Some(domain_count), Some(flags_count)) = (
                            program.buffers[*domain].count,
                            program.buffers[binding].count,
                        ) && flags_count < domain_count
                        {
                            return d.source.fail(
                                expr.span,
                                "B187",
                                "where 谓词容量不能小于原始域",
                            );
                        }
                        Ok(Selection { binding, word })
                    })
                    .transpose()?;
                let mut b = Builder::new(world, d, program, *domain);
                b.roots(env);
                let index = b.reg(Scalar::U32);
                b.code.push(Inst::Index(index));
                b.global_index = index;
                b.refine_parallel_domain();
                if b.env.contains_key(name) {
                    return d
                        .source
                        .fail(statement.span, "B107", "迭代变量不能覆盖参数");
                }
                let element = if matches!(domain_access, Access::InOut | Access::Write) {
                    let ty = program.buffers[*domain].ty.clone();
                    let words = ty.components.iter().map(|x| x.1 / 4).collect();
                    Binding::Owned(OwnedBinding {
                        place: Place::Element {
                            ty,
                            binding: *domain,
                            index,
                            words,
                            writable: true,
                        },
                        proof: bounds::Proof::default(),
                        uniform: false,
                        initialization: None,
                    })
                } else {
                    Binding::Element {
                        binding: *domain,
                        index,
                        writable: false,
                    }
                };
                // 每份inout输入的当前元素均由真实调用者提供初值。
                for (binding, access) in &b.view_access {
                    if *access == Access::InOut {
                        b.initialized.extend(
                            program.buffers[*binding]
                                .ty
                                .components
                                .iter()
                                .map(|(_, offset)| Position::Element(*binding, offset / 4)),
                        );
                    }
                }
                b.env.insert(name.clone(), element);
                if let Some(limit) = &prefix {
                    b.reads.insert(limit.binding);
                    b.cross_access.insert(limit.binding);
                }
                b.prefix = prefix;
                let inputs = b.tile_inputs(body)?;
                b.block(&body[inputs..])?;
                let appended: Vec<_> = b
                    .appends
                    .iter()
                    .map(|(domain, count)| PrefixRelation {
                        domain: *domain,
                        count: *count,
                    })
                    .collect();
                for relation in &appended {
                    prefix::prove(
                        facts,
                        relation.domain,
                        relation.count,
                        &d.source,
                        statement.span,
                    )?;
                }
                let mut outputs = b.writes.clone();
                outputs.insert(*domain);
                for binding in outputs {
                    if b.view_access[&binding] != Access::Write
                        || b.appends.iter().any(|(output, _)| *output == binding)
                    {
                        continue;
                    }
                    for (_, offset) in &program.buffers[binding].ty.components {
                        if !b
                            .initialized
                            .contains(&Position::Element(binding, offset / 4))
                        {
                            return d.source.fail(
                                statement.span,
                                "B108",
                                format!(
                                    "write 数据集 {} 有字段未在所有路径初始化",
                                    program.buffers[binding].name
                                ),
                            );
                        }
                    }
                }
                let tiles = b.lower_tiles(statement.span)?;
                if selection.is_some() && !tiles.is_empty() {
                    return d.source.fail(
                        statement.span,
                        "B250",
                        "where 稀疏域尚未定义 tile 邻接与发布语义",
                    );
                }
                if let Some(selection) = selection {
                    b.reads.insert(selection.binding);
                    b.cross_access.insert(selection.binding);
                }
                let stage = Stage {
                    domain: *domain,
                    prefix,
                    selection,
                    operation: Operation::Map,
                    tiles,
                    boundary_before: std::mem::take(boundary),
                    registers: b.registers,
                    locals: b.locals,
                    code: b.code,
                    reads: b.reads.into_iter().collect(),
                    writes: b.writes.into_iter().collect(),
                    cross_access: b.cross_access.into_iter().collect(),
                };
                facts.written(&stage.writes);
                for relation in appended {
                    facts.grant(relation);
                }
                program.stages.push(stage);
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args),
                ..
            }) if name == "clear_prefix" => {
                append::clear(program, env, args, d, statement.span, facts)?;
                *boundary = false;
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args),
                ..
            }) if name == "stable_sort_by_key" => {
                sort::compile(program, env, args, d, statement.span, facts)?;
                *boundary = false;
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args),
                ..
            }) if name == "compact" => {
                compact::compile(program, env, args, d, statement.span, facts)?;
                *boundary = false;
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args) | ExprKind::Specialize(name, _, args),
                ..
            }) => {
                let explicit = if let StmtKind::Expr(Expr {
                    kind: ExprKind::Specialize(_, types, _),
                    ..
                }) = &statement.kind
                {
                    Some(types.as_slice())
                } else {
                    None
                };
                if let Some(operator) =
                    CollectiveOperator::from_reduction_source(name).filter(|_| explicit.is_none())
                {
                    reduce::compile(program, env, operator, args, d, statement.span, facts)?;
                    *boundary = false;
                    continue;
                }
                if let Some(kind) =
                    custom_reduce::Kind::from_source(name).filter(|_| explicit.is_none())
                {
                    custom_reduce::compile(
                        world,
                        program,
                        env,
                        args,
                        d,
                        statement.span,
                        facts,
                        name.ends_with("_map"),
                        kind,
                    )?;
                    *boundary = false;
                    continue;
                }
                if let Some(operator) = CollectiveOperator::from_segmented_reduction_source(name)
                    .filter(|_| explicit.is_none())
                {
                    segmented_reduce::compile(
                        program,
                        env,
                        operator,
                        args,
                        d,
                        statement.span,
                        facts,
                    )?;
                    *boundary = false;
                    continue;
                }
                if let Some(operator) =
                    CollectiveOperator::from_scan_source(name).filter(|_| explicit.is_none())
                {
                    scan::compile(
                        program,
                        env,
                        operator,
                        false,
                        args,
                        d,
                        statement.span,
                        facts,
                    )?;
                    *boundary = false;
                    continue;
                }
                if let Some(operator) = CollectiveOperator::from_segmented_scan_source(name)
                    .filter(|_| explicit.is_none())
                {
                    scan::compile(program, env, operator, true, args, d, statement.span, facts)?;
                    *boundary = false;
                    continue;
                }
                let Some(key) = world.resolve(name, &d.module, &d.imports, true) else {
                    return d
                        .source
                        .fail(statement.span, "B109", format!("未知计算程序 {name}"));
                };
                let callee = &world.functions[&key];
                if callee.function.kind != FunctionKind::Program {
                    return d.source.fail(
                        statement.span,
                        "B110",
                        "program 顶层只能调用计算程序或声明 parallel 阶段",
                    );
                }
                if args.len() != callee.function.params.len() {
                    return d
                        .source
                        .fail(statement.span, "B111", "计算程序参数数量不匹配");
                }
                if program.target != callee.function.target
                    && callee.function.target != Target::Compute
                {
                    return d
                        .source
                        .fail(statement.span, "B112", "程序与子程序的设备目标不兼容");
                }
                let roots = args
                    .iter()
                    .map(|arg| stage_root(world, d, env, program, arg))
                    .collect::<Result<Vec<_>>>()?;
                let actuals: Vec<_> = roots
                    .iter()
                    .map(|root| match root {
                        Root::Data(i, _) => generics::Actual {
                            ty: Some(&program.buffers[*i].ty),
                            count: program.buffers[*i].count,
                        },
                        Root::Uniform(ty, _) | Root::Literal(ty, _) => generics::Actual::value(ty),
                        Root::Acceleration(_) => generics::Actual {
                            ty: None,
                            count: None,
                        },
                        Root::Texture(i) => generics::Actual {
                            ty: None,
                            count: program.textures[*i].count,
                        },
                    })
                    .collect();
                let instance =
                    generics::specialize(world, &key, &actuals, explicit, d, statement.span)?;
                let callee = &instance.definition;
                let mut child = BTreeMap::new();
                for ((p, arg), root) in callee.function.params.iter().zip(args).zip(roots) {
                    let expected = || {
                        world.ty(
                            &p.ty,
                            &callee.source,
                            p.span,
                            &callee.module,
                            &callee.imports,
                        )
                    };
                    match &root {
                        Root::Texture(i) => {
                            if !p.ty.is_texture_resource()
                                || p.access != Access::Read
                                || texture::shape(world, &p.ty, callee, p.span)?
                                    != program.textures[*i].count
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B239",
                                    "纹理资源与 read texture2d 参数不匹配",
                                );
                            }
                        }
                        Root::Acceleration(_) => {
                            if p.ty.name != "acceleration"
                                || p.ty.dataset
                                || p.access != Access::Read
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B182",
                                    "加速资源的类型或访问权限不匹配",
                                );
                            }
                        }
                        Root::Data(i, access) => {
                            let ty = expected()?;
                            if !p.ty.dataset || ty != program.buffers[*i].ty {
                                return d.source.fail(arg.span, "B115", "数据集类型不匹配");
                            }
                            if p.ty.count.is_some()
                                && world.shape(
                                    &p.ty.count,
                                    &callee.source,
                                    p.span,
                                    &callee.module,
                                    &callee.imports,
                                )? != program.buffers[*i].count
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B167",
                                    "调用方没有声明子程序需要的数据形状",
                                );
                            }
                            if (p.access.reads() && !access.reads())
                                || (p.access.writes() && !access.writes())
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B116",
                                    "子程序请求了调用方没有的数据访问权限",
                                );
                            }
                        }
                        Root::Uniform(t, _) | Root::Literal(t, _) => {
                            let ty = expected()?;
                            if p.ty.dataset || *t != ty {
                                return d.source.fail(arg.span, "B117", "标量参数类型不匹配");
                            }
                        }
                    }
                    child.insert(
                        p.name.clone(),
                        if let Root::Data(i, _) = root {
                            Root::Data(i, p.access)
                        } else {
                            root
                        },
                    );
                }
                let mut used = BTreeMap::new();
                for p in &callee.function.params {
                    if let Some(Root::Data(i, _)) = child.get(&p.name) {
                        if let Some(previous) = used.insert(*i, p.access) {
                            if (previous.writes() || p.access.writes())
                                && !(previous == Access::Atomic && p.access == Access::Atomic)
                            {
                                return d.source.fail(
                                    statement.span,
                                    "B118",
                                    "同一数据集通过多个参数别名进入可写子程序",
                                );
                            }
                        }
                    }
                }
                let required = prefix::relations(program, &child, callee, ContractKind::Requires)?;
                for relation in &required {
                    prefix::prove(
                        facts,
                        relation.domain,
                        relation.count,
                        &d.source,
                        statement.span,
                    )?;
                }
                let mut child_facts = prefix::Facts::new(&required);
                expand_program(
                    world,
                    callee,
                    &child,
                    program,
                    stack,
                    boundary,
                    &mut child_facts,
                )?;
                let writable: Vec<_> = callee
                    .function
                    .params
                    .iter()
                    .filter(|p| p.access.writes())
                    .filter_map(|p| {
                        if let Some(Root::Data(binding, _)) = child.get(&p.name) {
                            Some(*binding)
                        } else {
                            None
                        }
                    })
                    .collect();
                facts.written(&writable);
                for relation in prefix::relations(program, &child, callee, ContractKind::Ensures)? {
                    facts.grant(relation);
                }
            }
            StmtKind::Phase => {
                *boundary = true;
            }
            _ => {
                return d.source.fail(
                    statement.span,
                    "B119",
                    "program 顶层表达阶段；局部控制流放在 parallel 内",
                );
            }
        }
    }
    Ok(())
}
fn stage_root(
    world: &World,
    d: &Arc<Definition>,
    env: &BTreeMap<String, Root>,
    program: &Program,
    arg: &Expr,
) -> Result<Root> {
    match &arg.kind {
        ExprKind::Name(n) => {
            if let Some(root) = env.get(n) {
                Ok(root.clone())
            } else if let Some(key) = world
                .resolve(n, &d.module, &d.imports, true)
                .filter(|key| world.functions[key].function.kind == FunctionKind::Constant)
            {
                let (ty, words) = constant_value(world, &key)?;
                Ok(Root::Literal(ty, words))
            } else {
                d.source
                    .fail(arg.span, "B113", format!("未知参数或 const {n}"))
            }
        }
        ExprKind::StaticU32(bits) => Ok(Root::Literal(Ty::scalar(Scalar::U32), vec![*bits])),
        ExprKind::Number(n) => {
            let (ty, bits) = number(&d.source, arg.span, n)?;
            Ok(Root::Literal(ty, vec![bits]))
        }
        _ => constant::stage_expression(world, d, env, program, arg),
    }
}
fn collective_field(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
    scalar: Scalar,
) -> Result<(usize, u32, Access)> {
    let (binding, word, access, ty) = collective_value_field(program, env, expr, d)?;
    if ty != Ty::scalar(scalar) {
        return d
            .source
            .fail(expr.span, "B175", format!("集合运算字段需要 {scalar:?}"));
    }
    Ok((binding, word, access))
}
fn collective_value_field(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
) -> Result<(usize, u32, Access, Ty)> {
    let mut fields = Vec::new();
    let mut base = expr;
    while let ExprKind::Member(parent, field) = &base.kind {
        fields.push(field.as_str());
        base = parent;
    }
    if fields.is_empty() {
        return d
            .source
            .fail(expr.span, "B172", "集合运算参数必须是 dataset.field");
    }
    let ExprKind::Name(name) = &base.kind else {
        return d
            .source
            .fail(expr.span, "B172", "集合运算参数必须是命名数据集字段");
    };
    let Some(Root::Data(binding, access)) = env.get(name) else {
        return d.source.fail(expr.span, "B173", "未知集合运算数据集");
    };
    fields.reverse();
    let Some((offset, ty)) = program.buffers[*binding].ty.field(&fields.join(".")) else {
        return d.source.fail(expr.span, "B174", "未知集合运算字段");
    };
    Ok((*binding, offset / 4, *access, ty.clone()))
}
pub(crate) fn validate_signature(d: &Definition) -> Result<()> {
    if d.function.external.is_some() {
        return external::validate_signature(d);
    }
    if d.function.kind == FunctionKind::Program {
        return Ok(());
    }
    if d.function.result.dataset
        && !(d.function.kind == FunctionKind::Value && d.function.target == Target::Cpu)
    {
        return d.source.fail(
            d.function.span,
            "B224",
            "拥有的 dataset 返回属于普通 CPU 应用函数，借用与并行实例不转移资源所有权",
        );
    }
    if d.function.kind == FunctionKind::Cooperative && !d.function.params.iter().any(|p| p.ty.tile)
    {
        return d.source.fail(
            d.function.span,
            "B207",
            "cooperative 接口需要至少一个 tile<T> 借用来声明块范围",
        );
    }
    for p in &d.function.params {
        if p.ty.is_acceleration() {
            if p.access != Access::Read || d.function.target != Target::Gpu {
                return d
                    .source
                    .fail(p.span, "B182", "acceleration 必须是 GPU 函数的 read 资源");
            }
            continue;
        }
        if p.ty.dataset
            && !matches!(p.access, Access::Read | Access::Atomic)
            && !(d.function.kind == FunctionKind::Value
                && d.function.target == Target::Cpu
                && matches!(p.access, Access::Value | Access::InOut))
        {
            return d.source.fail(
                p.span,
                "B120",
                "函数 dataset 参数支持 read／atomic，CPU 应用函数另可按值取得所有权或 inout 借用；write 的全域初始化尚未实现",
            );
        }
        if p.ty.is_texture_resource() && p.access != Access::Read {
            return d
                .source
                .fail(p.span, "B239", "texture2d 必须是 read 借用参数");
        }
        if p.ty.tile
            && (d.function.kind != FunctionKind::Cooperative
                || !matches!(p.access, Access::Value | Access::Atomic))
        {
            return d.source.fail(
                p.span,
                "B207",
                "tile<T> 只能是 cooperative 的只读或 atomic 借用参数",
            );
        }
        if !p.ty.dataset && !p.ty.tile && !p.ty.is_texture_resource() && p.access == Access::Read {
            return d.source.fail(
                p.span,
                "B120",
                "函数参数使用普通值、write／inout 独占位置、read dataset 或 atomic 借用",
            );
        }
    }
    Ok(())
}
pub(crate) fn check_function(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let name = &instance.name;
    let d = &instance.definition;
    if d.function.external.is_some() {
        return external::check_declaration(world, d);
    }
    if d.function.kind == FunctionKind::Constant {
        return constant_value(world, name).map(|_| ());
    }
    // 独立函数按接口检查资源；实际内联仍绑定调用方的缓冲身份。
    let mut buffers = Vec::new();
    for p in &d.function.params {
        if p.ty.dataset {
            let ty = cooperative::parameter_type(world, p, d)?;
            buffers.push(Buffer {
                name: p.name.clone(),
                access: p.access,
                stride: ir::align_up(ty.size, ty.align),
                ty,
                count: world.shape(&p.ty.count, &d.source, p.span, &d.module, &d.imports)?,
            });
        }
    }
    let program = Program {
        name: name.clone(),
        target: d.function.target,
        uniform_bytes: 8 + 4 * buffers.len() as u32,
        buffers,
        accelerations: d
            .function
            .params
            .iter()
            .filter(|p| p.ty.is_acceleration())
            .map(|p| p.name.clone())
            .collect(),
        textures: d
            .function
            .params
            .iter()
            .filter(|p| p.ty.is_texture_resource())
            .map(|p| {
                Ok(TextureBinding {
                    name: p.name.clone(),
                    count: texture::shape(world, &p.ty, d, p.span)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        uniforms: Vec::new(),
        source_stages: 0,
        stages: Vec::new(),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(),
    };
    let mut b = Builder::new(world, d, &program, 0);
    b.in_function = true;
    if d.function.kind == FunctionKind::Value && d.function.target == Target::Cpu {
        b.application = Some(application::Context::default());
    }
    b.calls.push(name.into());
    if d.function.kind == FunctionKind::Cooperative {
        b.cooperative_root = Some(0);
    }
    let mut data_index = 0;
    let mut texture_index = 0;
    let mut acceleration_index = 0;
    for p in &d.function.params {
        if p.ty.is_acceleration() {
            b.env
                .insert(p.name.clone(), Binding::Acceleration(acceleration_index));
            acceleration_index += 1;
            continue;
        }
        if p.ty.is_texture_resource() {
            b.env.insert(
                p.name.clone(),
                Binding::Texture(texture::TextureReference::binding(texture_index)),
            );
            texture_index += 1;
            continue;
        }
        if p.ty.dataset {
            let binding = if d.function.kind == FunctionKind::Value
                && d.function.target == Target::Cpu
                && matches!(p.access, Access::Read | Access::InOut | Access::Value)
            {
                Binding::ApplicationData(b.application_parameter(data_index, p.access)?)
            } else {
                b.view_access.insert(data_index, p.access);
                Binding::Data(b.dataset_view(data_index))
            };
            b.env.insert(p.name.clone(), binding);
            data_index += 1;
            continue;
        }
        if p.ty.tile {
            let ty = cooperative::parameter_type(world, p, d)?;
            let tile = b.tiles.len();
            b.tiles.push(CheckedTile {
                source: TileOrigin::Parameter,
                ty,
                width: TileWidth::Parameter,
                access: p.access,
            });
            b.bind_tile(&p.name, tile);
            continue;
        }
        if p.access != Access::Value {
            if matches!(p.access, Access::InOut | Access::Write) {
                let ty = cooperative::parameter_type(world, p, d)?;
                let (slots, storage) = b.local_slots(&ty);
                let place = Place::Local(ty, slots, storage);
                if p.access == Access::InOut {
                    b.initialize(&place);
                }
                b.env.insert(
                    p.name.clone(),
                    Binding::Owned(OwnedBinding {
                        place,
                        proof: bounds::Proof::default(),
                        uniform: false,
                        initialization: None,
                    }),
                );
                continue;
            }
            if p.access == Access::Atomic {
                let ty = cooperative::parameter_type(world, p, d)?;
                b.env.insert(
                    p.name.clone(),
                    Binding::AtomicBorrow(Place::AtomicParameter(ty)),
                );
                continue;
            }
            return d.source.fail(
                p.span,
                "B120",
                "函数参数使用普通值、write／inout 独占位置、read dataset 或 atomic 借用",
            );
        }
        let ty = world.ty(&p.ty, &d.source, p.span, &d.module, &d.imports)?;
        let regs = ty.components.iter().map(|(s, _)| b.reg(*s)).collect();
        b.env.insert(
            p.name.clone(),
            Binding::Value(Value {
                proof: bounds::Proof::default(),
                literal: false,
                ty,
                regs,
                uniform: false,
                constant: None,
            }),
        );
    }
    let requirements = contracts::resolve(&b, d, &b.env)?;
    contracts::assume(&mut b.env, &requirements);
    b.materialize_array_bindings();
    b.function_body(d.function.body.as_ref().unwrap())?;
    Ok(())
}
fn number(source: &Source, span: Span, n: &str) -> Result<(Ty, u32)> {
    if !n.starts_with("0x")
        && (n.contains('.') || n.contains('e') || n.contains('E') || n.ends_with('f'))
    {
        let value = n
            .trim_end_matches('f')
            .parse::<f32>()
            .map_err(|_| Diagnostic {
                source: source.clone(),
                span,
                code: "B121",
                message: format!("非法 f32 常量 {n}"),
            })?;
        if !value.is_finite() {
            return source.fail(span, "B122", "浮点常量超出有限 f32 范围");
        }
        Ok((Ty::scalar(Scalar::F32), value.to_bits()))
    } else {
        let text = n.trim_end_matches('u');
        let bits = if let Some(hex) = text.strip_prefix("0x") {
            u32::from_str_radix(hex, 16)
        } else {
            text.parse::<u32>()
        }
        .map_err(|_| Diagnostic {
            source: source.clone(),
            span,
            code: "B123",
            message: format!("非法 u32 常量 {n}"),
        })?;
        Ok((Ty::scalar(Scalar::U32), bits))
    }
}
struct Builder<'a> {
    application: Option<application::Context>,
    world: &'a World,
    definition: Arc<Definition>,
    program: &'a Program,
    domain: usize,
    element_domains: BTreeSet<usize>,
    env: BTreeMap<String, Binding>,
    registers: Vec<Scalar>,
    locals: Vec<Scalar>,
    code: Vec<Inst>,
    global_index: Reg,
    reads: BTreeSet<usize>,
    writes: BTreeSet<usize>,
    cross_access: BTreeSet<usize>,
    view_access: BTreeMap<usize, Access>,
    initialized: BTreeSet<Position>,
    output_scopes: usize,
    control_uniform: bool,
    loop_depth: usize,
    block_depth: usize,
    in_function: bool,
    flow: Flow,
    loop_contexts: Vec<LoopContext>,
    return_context: Option<returns::ReturnContext>,
    cooperative_root: Option<usize>,
    calls: Vec<String>,
    iterators: Vec<Option<(String, usize)>>,
    mutable_names: BTreeSet<String>,
    tiles: Vec<CheckedTile>,
    appends: BTreeMap<usize, PrefixCount>,
    prefix: Option<PrefixCount>,
}
impl<'a> Builder<'a> {
    fn new(
        world: &'a World,
        definition: &Arc<Definition>,
        program: &'a Program,
        domain: usize,
    ) -> Self {
        let mut mutable_names = BTreeSet::new();
        assigned_names(
            world,
            definition,
            definition.function.body.as_ref().unwrap(),
            &mut mutable_names,
        );
        Self {
            application: None,
            world,
            definition: definition.clone(),
            program,
            domain,
            element_domains: BTreeSet::from([domain]),
            env: BTreeMap::new(),
            registers: Vec::new(),
            locals: Vec::new(),
            code: Vec::new(),
            global_index: usize::MAX,
            reads: BTreeSet::new(),
            writes: BTreeSet::new(),
            cross_access: BTreeSet::new(),
            view_access: BTreeMap::new(),
            initialized: BTreeSet::new(),
            output_scopes: 0,
            control_uniform: true,
            loop_depth: 0,
            block_depth: 0,
            in_function: false,
            flow: Flow::NEXT,
            loop_contexts: Vec::new(),
            return_context: None,
            cooperative_root: None,
            calls: Vec::new(),
            iterators: Vec::new(),
            mutable_names,
            tiles: Vec::new(),
            appends: BTreeMap::new(),
            prefix: None,
        }
    }
    fn roots(&mut self, env: &BTreeMap<String, Root>) {
        for (name, root) in env {
            match root {
                Root::Texture(i) => {
                    self.env.insert(
                        name.clone(),
                        Binding::Texture(texture::TextureReference::binding(*i)),
                    );
                }
                Root::Acceleration(i) => {
                    self.env.insert(name.clone(), Binding::Acceleration(*i));
                }
                Root::Data(i, access) => {
                    let view = self.dataset_view(*i);
                    self.env.insert(name.clone(), Binding::Data(view));
                    self.view_access.insert(*i, *access);
                }
                Root::Uniform(ty, offset) => {
                    let mut regs = Vec::new();
                    for (s, o) in &ty.components {
                        let r = self.reg(*s);
                        self.code.push(Inst::Uniform(r, offset + o));
                        regs.push(r);
                    }
                    self.env.insert(
                        name.clone(),
                        Binding::Value(Value {
                            proof: bounds::Proof::default(),
                            literal: false,
                            ty: ty.clone(),
                            regs,
                            uniform: true,
                            constant: None,
                        }),
                    );
                }
                Root::Literal(ty, bits) => {
                    let value = self.constant_value(ty.clone(), bits.clone());
                    self.env.insert(name.clone(), Binding::Value(value));
                }
            }
        }
    }
    fn fail<T>(&self, span: Span, code: &'static str, message: impl Into<String>) -> Result<T> {
        self.definition.source.fail(span, code, message)
    }
    fn reg(&mut self, ty: Scalar) -> Reg {
        let r = self.registers.len();
        self.registers.push(ty);
        r
    }
    fn constant(&mut self, s: Scalar, bits: u32) -> Value {
        let r = self.reg(s);
        self.code.push(Inst::Constant(r, s, bits));
        Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(s),
            regs: vec![r],
            uniform: true,
            constant: Some(vec![bits]),
        }
    }
    fn require_same(&self, a: &Ty, b: &Ty, span: Span) -> Result<()> {
        if a == b {
            Ok(())
        } else {
            self.fail(
                span,
                "B124",
                format!("类型不匹配：需要 {}，实际为 {}", a.name, b.name),
            )
        }
    }
    fn require_bool(&self, v: &Value, span: Span) -> Result<()> {
        self.require_same(&Ty::scalar(Scalar::Bool), &v.ty, span)
    }
    fn block(&mut self, body: &[Stmt]) -> Result<()> {
        let original = self.env.keys().cloned().collect::<BTreeSet<_>>();
        self.block_depth += 1;
        let mut flow = Flow::NEXT;
        for s in body {
            if !flow.continues() {
                break;
            }
            self.flow = Flow::NEXT;
            self.statement(s)?;
            flow = flow.then(self.flow);
        }
        self.flow = flow;
        self.block_depth -= 1;
        if flow.continues() {
            self.release_application_data(Some(&original));
        }
        self.env.retain(|k, _| original.contains(k));
        Ok(())
    }
    fn statement(&mut self, s: &Stmt) -> Result<()> {
        match &s.kind {
            StmtKind::Declare(annotation, name) => {
                return self.declare_statement(annotation, name, s.span);
            }
            StmtKind::Let(annotation, name, e) => {
                return self.let_statement(annotation, name, e, s.span);
            }
            StmtKind::Assign(lhs, op, rhs) => return self.assign_statement(lhs, op, rhs, s.span),
            StmtKind::Expr(e) => return self.expression_statement(e),
            StmtKind::If(condition, yes_body, no_body) => {
                return self.if_statement(condition, yes_body, no_body);
            }
            StmtKind::While(condition, body) => {
                return self.while_statement(condition, body);
            }
            StmtKind::For(name, start, end, body) => {
                return self.for_statement(name, start, end, body, s.span);
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    return self.fail(s.span, "B129", "break 只能用于局部顺序循环");
                }
                self.release_application_loop();
                self.code.push(Inst::Break);
                self.capture_break();
                self.flow = Flow::BREAK;
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    return self.fail(s.span, "B129", "continue 只能用于局部顺序循环");
                }
                self.check_application_loop(s.span)?;
                self.release_application_loop();
                if let Some(Some((_, slot))) = self.iterators.last().cloned() {
                    let index = self.reg(Scalar::U32);
                    self.code.push(Inst::LoadLocal(index, slot));
                    self.advance_iterator(index, slot);
                }
                self.code.push(Inst::Continue);
                self.flow = Flow::CONTINUE;
            }
            StmtKind::Return(value) => self.return_statement(value.as_ref(), s.span)?,
            StmtKind::Parallel(..) => {
                return self.fail(
                    s.span,
                    "B131",
                    "当前版本的并行域位于 program 顶层；嵌套域需要明确协作语义，尚未实现",
                );
            }
            StmtKind::Phase => return self.fail(s.span, "B163", "phase 只能位于 program 阶段层"),
            StmtKind::Seek(name, position) => return self.seek_statement(name, position, s.span),
        }
        Ok(())
    }
    fn value_result(&self, result: Option<Value>, span: Span) -> Result<Value> {
        match result {
            Some(value) => Ok(value),
            None => self.fail(span, "B228", "此处需要普通值，void 调用没有返回值"),
        }
    }
    fn place(&mut self, e: &Expr) -> Result<Place> {
        match &e.kind {
            ExprKind::Name(name) => self.name_place(name, e),
            ExprKind::Member(base, field) => self.member_place(base, field, e),
            ExprKind::Index(base, index) => self.index_place(base, index),
            _ => self.fail(e.span, "B138", "表达式不是可赋值位置"),
        }
    }
    fn member(&self, ty: &Ty, field: &str, span: Span) -> Result<(Ty, Vec<usize>)> {
        if ty.view.is_some() {
            return if field == "length" {
                Ok((Ty::scalar(Scalar::U32), vec![2]))
            } else {
                self.fail(span, "B185", "view 只公开只读 length，不公开设备地址分量")
            };
        }
        if let Some((_, offset, sub)) = ty.fields.iter().find(|(n, _, _)| n == field) {
            let indices = sub
                .components
                .iter()
                .map(|(_, o)| {
                    ty.components
                        .iter()
                        .position(|(_, v)| *v == offset + o)
                        .unwrap()
                })
                .collect();
            return Ok(((**sub).clone(), indices));
        }
        if ty.array.is_none() && ty.fields.is_empty() && ty.components.len() > 1 && field.len() <= 4
        {
            let mut indices = Vec::new();
            for c in field.chars() {
                let i = match c {
                    'x' | 'r' => 0,
                    'y' | 'g' => 1,
                    'z' | 'b' => 2,
                    'w' | 'a' => 3,
                    _ => return self.fail(span, "B139", format!("未知字段 {field}")),
                };
                if i >= ty.components.len() {
                    return self.fail(span, "B140", "向量分量越界");
                }
                indices.push(i);
            }
            return Ok((Ty::vector(ty.components[0].0, indices.len()), indices));
        }
        self.fail(span, "B141", format!("{} 没有字段 {field}", ty.name))
    }
    fn read_place(&mut self, p: &Place, span: Span) -> Result<Value> {
        match p {
            Place::ApplicationElement {
                ty,
                handle,
                index,
                stride,
                words,
                ..
            } => self.read_application_place(ty, *handle, *index, *stride, words, span),
            Place::Indexed { .. } => self.read_array_place(p, span),
            Place::AtomicParameter(_) => {
                self.fail(span, "B214", "原子借用只能通过显式原子操作读取")
            }
            Place::ViewElement {
                ty,
                address,
                index,
                stride,
                words,
            } => {
                let mut regs = Vec::new();
                for ((scalar, _), word) in ty.components.iter().zip(words) {
                    let dst = self.reg(*scalar);
                    self.code.push(Inst::ViewLoad {
                        dst,
                        address: *address,
                        index: *index,
                        stride: *stride,
                        word: *word,
                    });
                    regs.push(dst);
                }
                Ok(Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform: false,
                    constant: None,
                })
            }
            Place::TileElement {
                ty,
                tile,
                index,
                words,
            } => {
                if self.tiles[*tile].access == Access::Atomic {
                    return self.fail(span, "B214", "atomic tile 元素只能通过显式原子操作读取");
                }
                if let TileOrigin::Bound(TileSource::Input { binding, .. }) =
                    self.tiles[*tile].source
                {
                    self.reads.insert(binding);
                    self.cross_access.insert(binding);
                }
                let mut regs = Vec::new();
                for ((scalar, _), word) in ty.components.iter().zip(words) {
                    let dst = self.reg(*scalar);
                    self.code.push(Inst::TileLoad {
                        dst,
                        tile: *tile,
                        index: *index,
                        word: *word,
                    });
                    regs.push(dst);
                }
                Ok(Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform: false,
                    constant: None,
                })
            }
            Place::Local(ty, slots, ..) | Place::ReadOnlyLocal(ty, slots, ..) => {
                if matches!(p, Place::Local(..)) {
                    self.require_initialized(p, span)?;
                }
                let mut regs = Vec::new();
                for (s, slot) in ty.components.iter().zip(slots) {
                    let r = self.reg(s.0);
                    self.code.push(Inst::LoadLocal(r, *slot));
                    regs.push(r);
                }
                let (proof, uniform) = self.read_owned_proof(p, &regs);
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform,
                    constant: None,
                })
            }
            Place::Element {
                ty,
                binding,
                index,
                words,
                ..
            } => {
                if self.view_access[binding] == Access::Atomic {
                    return self.fail(span, "B214", "atomic 数据集元素只能通过显式原子操作读取");
                }
                if self.view_access[binding].writes() {
                    self.require_initialized(p, span)?;
                }
                self.reads.insert(*binding);
                if *index != self.global_index {
                    self.cross_access.insert(*binding);
                }
                let mut regs = Vec::new();
                for ((s, _), word) in ty.components.iter().zip(words) {
                    let dst = self.reg(*s);
                    self.code.push(Inst::Load {
                        dst,
                        binding: *binding,
                        index: *index,
                        word: *word,
                    });
                    regs.push(dst);
                }
                let proof = if self.view_access[binding].writes() {
                    self.read_owned_proof(p, &regs).0
                } else {
                    bounds::Proof::default()
                };
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform: false,
                    constant: None,
                })
            }
        }
    }
    fn write_place(&mut self, p: &Place, value: Value, span: Span) -> Result<()> {
        let ty = p.ty();
        self.require_same(ty, &value.ty, span)?;
        match p {
            Place::ApplicationElement {
                handle,
                index,
                stride,
                words,
                ..
            } => {
                self.writable_storage(p, span)?;
                return self.write_application_place(*handle, *index, *stride, words, value, span);
            }
            Place::Indexed { .. } => return self.write_array_place(p, value, span),
            Place::ReadOnlyLocal(..) => {
                return self.fail(span, "B144", "按值参数或不可变局部值不可写");
            }
            Place::AtomicParameter(_) => {
                return self.fail(span, "B214", "原子借用只能通过显式原子操作修改");
            }
            Place::ViewElement { .. } => return self.fail(span, "B185", "view 的元素不可写"),
            Place::TileElement { tile, .. } => {
                if self.tiles[*tile].access == Access::Atomic {
                    return self.fail(span, "B214", "atomic tile 元素只能通过显式原子操作修改");
                }
                return self.fail(span, "B202", "tile 值不可写；只修改当前独占的全局元素");
            }
            Place::Local(_, slots, ..) => {
                if slots.iter().copied().collect::<BTreeSet<_>>().len() != slots.len() {
                    return self.fail(span, "B143", "赋值位置包含重复分量");
                }
                self.assign_owned_proof(p, bounds::Proof::capture(&value), false);
                self.initialize(p);
                for (slot, value) in slots.iter().zip(value.regs) {
                    self.code.push(Inst::StoreLocal(*slot, value));
                }
            }
            Place::Element {
                binding,
                index,
                words,
                writable,
                ..
            } => {
                if self.view_access[binding] == Access::Atomic {
                    return self.fail(span, "B214", "atomic 数据集元素只能通过显式原子操作修改");
                }
                let b = &self.program.buffers[*binding];
                if !*writable || !self.view_access[binding].writes() {
                    return self.fail(
                        span,
                        "B144",
                        format!("没有 {} 当前元素的独占修改权", b.name),
                    );
                }
                if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
                    return self.fail(span, "B143", "赋值位置包含重复分量");
                }
                self.writes.insert(*binding);
                self.assign_owned_proof(p, bounds::Proof::capture(&value), false);
                for (word, value) in words.iter().zip(value.regs) {
                    self.code.push(Inst::Store {
                        binding: *binding,
                        index: *index,
                        word: *word,
                        value,
                    });
                }
                self.initialize(p);
            }
        }
        Ok(())
    }
    fn expr(&mut self, e: &Expr) -> Result<Value> {
        match &e.kind {
            ExprKind::FunctionRef(..) => self.fail(
                e.span,
                "B236",
                "静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值",
            ),
            ExprKind::Cooperate(..) => {
                self.fail(e.span, "B208", "cooperate 调用只能作为协作根块的局部声明")
            }
            ExprKind::StaticU32(bits) => Ok(self.constant(Scalar::U32, *bits)),
            ExprKind::Specialize(name, explicit, args) => {
                let value = self.specialized_call(name, explicit, args, e.span)?;
                self.value_result(value, e.span)
            }
            ExprKind::Number(n) => {
                let (t, b) = number(&self.definition.source, e.span, n)?;
                Ok(self.literal(t.components[0].0, b))
            }
            ExprKind::Bool(b) => Ok(self.constant(Scalar::Bool, *b as u32)),
            ExprKind::Name(name) => self.name_expr(name, e),
            ExprKind::Member(base, field) => self.member_expr(base, field, e),
            ExprKind::Index(base, index) => self.index_expr(base, index, e),
            ExprKind::Unary(op, a) => self.unary_expr(op, a, e.span),
            ExprKind::Binary(op, a, b) => self.binary_expr(op, a, b, e.span),
            ExprKind::Select(c, a, b) => self.selection(c, a, b, e.span),
            ExprKind::Construct(ty, args) => {
                let values = args
                    .iter()
                    .map(|arg| self.expr(arg))
                    .collect::<Result<Vec<_>>>()?;
                self.record_value(ty, values, e.span)
            }
            ExprKind::Call(name, args) => {
                let result = self.call(name, args, e.span)?;
                self.value_result(result, e.span)
            }
        }
    }
    fn bitcast(&mut self, name: &str, mut args: Vec<Value>, span: Span) -> Result<Value> {
        if args.len() != 1 {
            return self.fail(span, "B153", format!("{name} 需要一个参数"));
        }
        let (source, target) = if name == "bitcast_u32" {
            (Scalar::F32, Scalar::U32)
        } else {
            (Scalar::U32, Scalar::F32)
        };
        let value = args.remove(0);
        let n = value.regs.len();
        if !(1..=4).contains(&n) || value.ty != Ty::vector(source, n) {
            return self.fail(span, "B154", format!("{name} 需要对应的32位标量或向量"));
        }
        let ty = Ty::vector(target, n);
        if let Some(bits) = value.constant {
            let mut result = self.constant_value(ty, bits);
            result.literal = false;
            return Ok(result);
        }
        let mut regs = Vec::with_capacity(n);
        for a in value.regs {
            let dst = self.reg(target);
            self.code.push(Inst::Unary {
                dst,
                op: UnaryOp::Bitcast,
                a,
            });
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: value.uniform,
            constant: None,
        })
    }
    fn cast(&mut self, v: Value, s: Scalar) -> Value {
        let source = v.ty.components[0].0;
        let constant = v.constant.as_ref().and_then(|values| {
            values
                .iter()
                .map(|bits| {
                    Some(match (source, s) {
                        (Scalar::F32, Scalar::U32) => {
                            let n = f32::from_bits(*bits);
                            if !(0.0..4294967296.0).contains(&n) {
                                return None;
                            }
                            n as u32
                        }
                        (Scalar::F32, Scalar::I32) => {
                            let n = f32::from_bits(*bits);
                            if !(-2147483648.0..2147483648.0).contains(&n) {
                                return None;
                            }
                            n as i32 as u32
                        }
                        (Scalar::F32, Scalar::Bool) => u32::from(f32::from_bits(*bits) != 0.0),
                        (Scalar::I32, Scalar::F32) => (*bits as i32 as f32).to_bits(),
                        (Scalar::U32 | Scalar::Bool, Scalar::F32) => (*bits as f32).to_bits(),
                        (_, Scalar::Bool) => u32::from(*bits != 0),
                        _ => *bits,
                    })
                })
                .collect::<Option<Vec<_>>>()
        });
        let proof = if source == s {
            bounds::Proof::capture(&v)
        } else if matches!(source, Scalar::I32 | Scalar::U32)
            && matches!(s, Scalar::I32 | Scalar::U32)
        {
            bounds::Proof::integer_bits(&v)
        } else if source == Scalar::Bool && s == Scalar::U32 {
            bounds::Proof::unsigned(0, 1)
        } else {
            bounds::Proof::default()
        };
        let mut regs = Vec::new();
        let n = v.regs.len();
        if source != s {
            if let Some(bits) = constant {
                let mut value = self.constant_value(Ty::vector(s, n), bits);
                value.proof = proof;
                return value;
            }
        }
        for src in v.regs {
            if self.registers[src] == s {
                regs.push(src);
            } else {
                let dst = self.reg(s);
                self.code.push(Inst::Cast { dst, src });
                regs.push(dst);
            }
        }
        Value {
            proof,
            literal: false,
            ty: Ty::vector(s, n),
            regs,
            uniform: v.uniform,
            constant,
        }
    }
    fn binary(&mut self, op: &str, mut a: Value, mut b: Value, span: Span) -> Result<Value> {
        if a.ty.matrix_shape().is_some() || b.ty.matrix_shape().is_some() {
            return self.matrix_binary(op, a, b, span);
        }
        if a.ty.array.is_some()
            || b.ty.array.is_some()
            || !a.ty.fields.is_empty()
            || !b.ty.fields.is_empty()
            || a.ty.contains_views()
            || b.ty.contains_views()
        {
            return self.fail(span, "B147", "结构体和资源视图不能作为数值进行运算");
        }
        let literal = a.literal && b.literal;
        if a.ty.components[0].0 != b.ty.components[0].0 {
            if literal {
                let scalar =
                    if a.ty.components[0].0 == Scalar::F32 || b.ty.components[0].0 == Scalar::F32 {
                        Scalar::F32
                    } else {
                        Scalar::I32
                    };
                a = self.cast(a, scalar);
                b = self.cast(b, scalar);
            } else if a.literal && b.ty.components[0].0 != Scalar::Bool {
                a = self.cast(a, b.ty.components[0].0);
            } else if b.literal && a.ty.components[0].0 != Scalar::Bool {
                b = self.cast(b, a.ty.components[0].0);
            } else {
                return self.fail(span, "B148", "不同数值类型必须显式转换");
            }
        }
        let n = a.regs.len().max(b.regs.len());
        if a.regs.len() != b.regs.len() && a.regs.len() != 1 && b.regs.len() != 1 {
            return self.fail(span, "B149", "向量维度不匹配");
        }
        let scalar = a.ty.components[0].0;
        let comparison = ["==", "!=", "<", ">", "<=", ">="].contains(&op);
        let literal = literal && scalar != Scalar::F32 && !comparison;
        if scalar == Scalar::Bool && !matches!(op, "==" | "!=") {
            return self.fail(span, "B150", "bool 只支持逻辑或相等运算");
        }
        if scalar == Scalar::F32 && matches!(op, "%" | "&" | "|" | "^" | "<<" | ">>") {
            return self.fail(span, "B151", "位运算与取余需要整数");
        }
        let divisors = if matches!(op, "/" | "%") && scalar != Scalar::F32 {
            match b.constant.as_ref().and_then(|v| {
                v.iter()
                    .copied()
                    .map(std::num::NonZeroU32::new)
                    .collect::<Option<Vec<_>>>()
            }) {
                Some(v) => Some(
                    v.into_iter()
                        .map(IntegerDivisor::Constant)
                        .collect::<Vec<_>>(),
                ),
                None if b.is_nonzero() => {
                    Some(b.regs.iter().copied().map(IntegerDivisor::Value).collect())
                }
                None => {
                    return self.fail(
                        span,
                        "B152",
                        "整数除数的每个分量都必须是非零常量或在当前作用域已证明非零的值",
                    );
                }
            }
        } else {
            None
        };
        let operation = if divisors.is_some() {
            None
        } else {
            Some(BinaryOp::from_source(op, scalar).ok_or_else(|| Diagnostic {
                source: self.definition.source.clone(),
                span,
                code: "B178",
                message: format!("未知二元运算 {op}"),
            })?)
        };
        let ty = Ty::vector(if comparison { Scalar::Bool } else { scalar }, n);
        let constant = a
            .constant
            .as_ref()
            .zip(b.constant.as_ref())
            .and_then(|(a, b)| {
                (0..n)
                    .map(|i| {
                        let left = a[i % a.len()];
                        let right = b[i % b.len()];
                        if let Some(divisors) = &divisors {
                            match divisors[i % divisors.len()] {
                                IntegerDivisor::Constant(divisor) => {
                                    Some(constant::divide(scalar, left, divisor, op == "%"))
                                }
                                IntegerDivisor::Value(_) => None,
                            }
                        } else {
                            constant::binary(operation.unwrap(), scalar, left, right)
                        }
                    })
                    .collect::<Option<Vec<_>>>()
            });
        let proof = if let Some(BinaryOp::Compare(op)) = operation {
            bounds::Proof::condition(bounds::Condition::comparison(op, &a, &b))
        } else {
            bounds::arithmetic(op, &a, &b)
        };
        if let Some(bits) = constant {
            let mut value = self.constant_value(ty, bits);
            value.literal = literal;
            value.proof = proof;
            return Ok(value);
        }
        let mut regs = Vec::new();
        for i in 0..n {
            let dst = self.reg(if comparison { Scalar::Bool } else { scalar });
            let ar = a.regs[i % a.regs.len()];
            let br = b.regs[i % b.regs.len()];
            if let Some(divisors) = &divisors {
                let divisor = divisors[i % divisors.len()];
                if scalar == Scalar::I32
                    && matches!(divisor, IntegerDivisor::Constant(d) if d.get() == u32::MAX)
                {
                    if op == "/" {
                        self.code.push(Inst::Unary {
                            dst,
                            op: UnaryOp::SignedNegate,
                            a: ar,
                        });
                    } else {
                        self.code.push(Inst::Constant(dst, Scalar::I32, 0));
                    }
                } else {
                    self.code.push(Inst::IntegerDivide {
                        result: if op == "/" {
                            IntegerDivisionResult::Quotient(dst)
                        } else {
                            IntegerDivisionResult::Remainder(dst)
                        },
                        src: ar,
                        divisor,
                    });
                }
            } else {
                self.code.push(Inst::Binary {
                    dst,
                    op: operation.unwrap(),
                    a: ar,
                    b: br,
                });
            }
            regs.push(dst);
        }
        Ok(Value {
            proof,
            literal,
            ty,
            regs,
            uniform: a.uniform && b.uniform,
            constant: None,
        })
    }
    fn math(&mut self, op: &str, args: Vec<Value>, span: Span) -> Result<Value> {
        let arity = if matches!(op, "min" | "max" | "pow") {
            2
        } else {
            1
        };
        if args.len() != arity {
            return self.fail(span, "B153", format!("{op} 需要 {arity} 个参数"));
        }
        let n = args.iter().map(|v| v.regs.len()).max().unwrap();
        for a in &args {
            if a.ty.components[0].0 != Scalar::F32
                || !a.ty.fields.is_empty()
                || a.ty.array.is_some()
                || (a.regs.len() != 1 && a.regs.len() != n)
            {
                return self.fail(span, "B154", format!("{op} 需要 f32 或相容 float 向量"));
            }
        }
        let uniform = args.iter().all(|v| v.uniform);
        let operation = MathOp::from_source(op).ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B179",
            message: format!("未知数学运算 {op}"),
        })?;
        let mut regs = Vec::new();
        for i in 0..n {
            let dst = self.reg(Scalar::F32);
            self.code.push(Inst::Math {
                dst,
                op: operation,
                args: args.iter().map(|a| a.regs[i % a.regs.len()]).collect(),
            });
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::vector(Scalar::F32, n),
            regs,
            uniform,
            constant: None,
        })
    }
    fn call(&mut self, name: &str, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if matches!(name, "upload" | "download") {
            return self.application_transfer(name, args, span);
        }
        if matches!(name, "run_cpu" | "run_gpu") {
            return self.application_run(name, args, span);
        }
        if name == "println" {
            return self.println(args, span).map(Some);
        }
        if name == "texture_sample" {
            return self.texture_sample(args, span).map(Some);
        }
        if name == "append" {
            return self.append(args, span).map(Some);
        }
        if TileOp::from_source(name).is_some() {
            return self.fail(
                span,
                "B205",
                "tile 集合运算只能作为 parallel 根块的局部变量声明",
            );
        }
        if name == "tile_publish" {
            return self.fail(
                span,
                "B203",
                "tile_publish 只能作为 parallel 根块的 let 发布声明",
            );
        }
        if name == "tile_input" {
            return self.fail(
                span,
                "B198",
                "tile_input 只能作为 parallel 开头的 let 输入声明，不能创建普通值或在控制流中构造",
            );
        }
        if let Some(value) = self.subgroup(name, args, span) {
            return value.map(Some);
        }
        if matches!(name, "trace_closest" | "trace_any") {
            return self.ray_query_call(name, args, span);
        }
        if name == "unsafe_read" {
            return self.unsafe_read_call(args, span);
        }
        if name == "ordinal" {
            return self.ordinal_call(args, span);
        }
        if name == "index" {
            if !args.is_empty() || self.in_function {
                return self.fail(
                    span,
                    "B155",
                    "index() 只能在当前 program 的 parallel 域中使用",
                );
            }
            return Ok(Some(Value {
                proof: self
                    .data_index_proof(self.domain)
                    .with_domains(Domains::from([IndexDomain::Data(self.domain)])),
                literal: false,
                ty: Ty::scalar(Scalar::U32),
                regs: vec![self.global_index],
                uniform: false,
                constant: None,
            }));
        }
        if matches!(
            name,
            "bind" | "barrier" | "group_barrier" | "compact" | "stable_sort_by_key" | "ddx" | "ddy"
        ) || custom_reduce::Kind::from_source(name).is_some()
            || CollectiveOperator::from_scan_source(name).is_some()
            || CollectiveOperator::from_segmented_scan_source(name).is_some()
            || CollectiveOperator::from_reduction_source(name).is_some()
            || CollectiveOperator::from_segmented_reduction_source(name).is_some()
        {
            return self.fail(
                span,
                "B156",
                format!("{name} 的作用域／能力语义尚未实现，当前编译器明确拒绝"),
            );
        }
        if name == "atomic_store" {
            return self.fail(span, "B218", "atomic_store 是语句，没有可绑定的返回值");
        }
        if name == "tile_barrier" {
            return self.fail(span, "B218", "tile_barrier 是语句，没有可绑定的返回值");
        }
        if matches!(
            name,
            "atomic_load"
                | "atomic_add"
                | "atomic_sub"
                | "atomic_exchange"
                | "atomic_compare_exchange"
                | "atomic_min"
                | "atomic_max"
                | "atomic_and"
                | "atomic_or"
                | "atomic_xor"
        ) {
            return self.atomic_value(name, args, span).map(Some);
        }
        let function_key = self.world.resolve(
            name,
            &self.definition.module,
            &self.definition.imports,
            true,
        );
        if let Some(key) = &function_key
            && (self.world.functions[key].function.external.is_some()
                || self.world.functions[key].function.params.iter().any(|p| {
                    p.ty.dataset
                        || p.ty.tile
                        || p.ty.is_acceleration()
                        || p.ty.is_texture_resource()
                        || (matches!(p.access, Access::Atomic | Access::InOut | Access::Write)
                            && !p.ty.tile)
                }))
        {
            let arguments = self.function_arguments(key, args, span)?;
            return self.invoke_function(key.clone(), arguments, span);
        }
        let mut values = Vec::new();
        for a in args {
            values.push(self.expr(a)?);
        }
        if let Some(ty) = Ty::builtin(name) {
            return self.numeric_constructor(ty, values, span).map(Some);
        }
        if self
            .world
            .resolve(
                name,
                &self.definition.module,
                &self.definition.imports,
                false,
            )
            .is_some()
        {
            return self
                .record_value(&TypeName::named(name), values, span)
                .map(Some);
        }
        if matches!(name, "bitcast_u32" | "bitcast_f32") {
            return self.bitcast(name, values, span).map(Some);
        }
        if matches!(
            name,
            "sqrt"
                | "abs"
                | "floor"
                | "sin"
                | "cos"
                | "exp"
                | "exp2"
                | "log"
                | "log2"
                | "min"
                | "max"
                | "pow"
        ) {
            return self.math(name, values, span).map(Some);
        }
        if matches!(name, "transpose" | "determinant" | "inverse") {
            return self.matrix_intrinsic(name, values, span).map(Some);
        }
        if name == "dot" {
            return self.dot_value(values, span).map(Some);
        }
        if matches!(name, "length" | "normalize") {
            return self
                .length_value(values, name == "normalize", span)
                .map(Some);
        }
        if name == "cross" {
            return self.cross_value(values, span).map(Some);
        }
        if name == "clamp" {
            if values.len() != 3 {
                return self.fail(span, "B153", "clamp 需要三个参数");
            }
            let a = self.math("max", vec![values[0].clone(), values[1].clone()], span)?;
            return self.math("min", vec![a, values[2].clone()], span).map(Some);
        }
        let Some(key) = function_key else {
            return self.fail(span, "B158", format!("未知函数或能力 {name}"));
        };
        self.invoke_function(
            key,
            values
                .into_iter()
                .map(cooperative::Argument::Value)
                .collect(),
            span,
        )
    }
    fn unary_expr(&mut self, op: &String, a: &Expr, span: Span) -> Result<Value> {
        if op == "-" {
            if let ExprKind::Number(literal) = &a.kind {
                let (ty, bits) = number(&self.definition.source, span, literal)?;
                return match ty.components[0].0 {
                    Scalar::F32 => Ok(self.literal(Scalar::F32, bits ^ 0x80000000)),
                    Scalar::U32 if bits <= 0x80000000 => {
                        Ok(self.literal(Scalar::I32, 0u32.wrapping_sub(bits)))
                    }
                    _ => self.fail(span, "B180", "负整数字面量超出 i32 范围"),
                };
            }
        }
        let a = self.expr(a)?;
        if op == "+" {
            if a.ty.array.is_some() {
                return self.fail(span, "B146", "数值一元运算不能接受数组");
            }
            return Ok(a);
        }
        if op == "!" {
            self.require_bool(&a, span)?;
        } else if !a.ty.is_numeric() && !(op == "-" && a.ty.matrix_shape().is_some()) {
            return self.fail(span, "B146", "数值一元运算需要数值类型");
        }
        let scalar = a.ty.components[0].0;
        let operation = match op.as_str() {
            "-" => match scalar {
                Scalar::F32 => UnaryOp::FloatNegate,
                Scalar::I32 => UnaryOp::SignedNegate,
                Scalar::U32 => UnaryOp::UnsignedNegate,
                Scalar::Bool => return self.fail(span, "B176", "bool 不能取负"),
            },
            "!" => UnaryOp::LogicalNot,
            "~" if scalar != Scalar::F32 => UnaryOp::BitNot,
            _ => return self.fail(span, "B177", "当前类型不支持该一元运算"),
        };
        let constant = a.constant.as_ref().map(|values| {
            values
                .iter()
                .map(|bits| match operation {
                    UnaryOp::FloatNegate => bits ^ 0x80000000,
                    UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => 0u32.wrapping_sub(*bits),
                    UnaryOp::LogicalNot => u32::from(*bits == 0),
                    UnaryOp::BitNot => !bits,
                    UnaryOp::Bitcast => *bits,
                })
                .collect()
        });
        let proof = bounds::unary(operation, &a);
        if let Some(bits) = constant {
            let mut value = self.constant_value(a.ty, bits);
            value.literal = a.literal;
            value.proof = proof;
            return Ok(value);
        }
        let mut regs = Vec::new();
        for x in a.regs {
            let dst = self.reg(self.registers[x]);
            self.code.push(Inst::Unary {
                dst,
                op: operation,
                a: x,
            });
            regs.push(dst);
        }
        Ok(Value {
            proof,
            literal: a.literal,
            ty: a.ty,
            regs,
            uniform: a.uniform,
            constant: None,
        })
    }
    fn binary_expr(&mut self, op: &String, a: &Expr, b: &Expr, span: Span) -> Result<Value> {
        let left = a;
        let a = self.expr(left)?;
        if op == "&&" || op == "||" {
            let mut modified = BTreeSet::new();
            expression_assignments(self.world, &self.definition, b, &mut modified);
            self.require_bool(&a, span)?;
            let outer = std::mem::take(&mut self.code);
            let environment = self.env.clone();
            let effects = (
                self.reads.clone(),
                self.writes.clone(),
                self.cross_access.clone(),
            );
            let before = self.initialized.clone();
            let control = self.control_uniform;
            self.control_uniform &= a.uniform;
            self.refine_bounds(&a, op == "&&");
            let b = self.expr(b)?;
            self.require_bool(&b, span)?;
            let after = std::mem::take(&mut self.initialized);
            if op == "&&" {
                self.join_initialization(&a, after, before);
            } else {
                self.join_initialization(&a, before, after);
            }
            let branch_env = self.env.clone();
            let branch_releases = self.application_join_releases(&branch_env, &environment);
            let skipped_releases = self.application_join_releases(&environment, &branch_env);
            self.restore_expression_environment(environment, &modified);
            self.refine_bounds(&a, op != "&&");
            let skipped_env = std::mem::replace(&mut self.env, branch_env);
            if let Some(bits) = &a.constant {
                if (bits[0] != 0) != (op == "&&") {
                    self.env = skipped_env;
                    (self.reads, self.writes, self.cross_access) = effects;
                }
            } else if op == "&&" {
                self.join_environment(&skipped_env, Some(&a));
            } else {
                let branch_env = std::mem::replace(&mut self.env, skipped_env);
                self.join_environment(&branch_env, Some(&a));
            }
            self.control_uniform = control;
            let mut branch = std::mem::take(&mut self.code);
            if a.constant.is_none() {
                branch.extend(branch_releases);
            }
            self.code = outer;
            if let Some(bits) = &a.constant {
                return if (bits[0] != 0) == (op == "&&") {
                    self.code.extend(branch);
                    let proof =
                        bounds::Proof::condition(bounds::Condition::logical(op == "&&", &a, &b));
                    let mut value = b;
                    value.proof = proof;
                    Ok(value)
                } else {
                    Ok(a)
                };
            }
            let slot = self.locals.len();
            self.locals.push(Scalar::Bool);
            self.code.push(Inst::StoreLocal(slot, a.regs[0]));
            branch.push(Inst::StoreLocal(slot, b.regs[0]));
            self.code.push(Inst::If {
                condition: a.regs[0],
                yes: if op == "&&" {
                    branch.clone()
                } else {
                    skipped_releases.clone()
                },
                no: if op == "||" { branch } else { skipped_releases },
            });
            let r = self.reg(Scalar::Bool);
            self.code.push(Inst::LoadLocal(r, slot));
            return Ok(Value {
                proof: bounds::Proof::condition(bounds::Condition::logical(op == "&&", &a, &b)),
                literal: false,
                ty: Ty::scalar(Scalar::Bool),
                regs: vec![r],
                uniform: a.uniform && b.uniform,
                constant: None,
            });
        }
        let b = self.expr(b)?;
        self.binary(op, a, b, span)
    }
    fn if_statement(
        &mut self,
        condition: &Expr,
        yes_body: &[Stmt],
        no_body: &[Stmt],
    ) -> Result<()> {
        let c = self.expr(condition)?;
        self.require_bool(&c, condition.span)?;
        if let Some(bits) = &c.constant {
            let truth = bits[0] != 0;
            self.refine_bounds(&c, truth);
            return self.block(if truth { yes_body } else { no_body });
        }
        let environment = self.env.clone();
        let previous = std::mem::take(&mut self.code);
        let init = self.initialized.clone();
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.refine_bounds(&c, true);
        self.block(yes_body)?;
        let mut yes = std::mem::take(&mut self.code);
        let yes_init = self.initialized.clone();
        let yes_env = self.env.clone();
        let yes_flow = self.flow;
        let yes_reachable = yes_flow.continues();
        self.flow = Flow::NEXT;
        self.initialized = init;
        self.env = environment.clone();
        self.refine_bounds(&c, false);
        self.block(no_body)?;
        let mut no = std::mem::take(&mut self.code);
        let no_init = std::mem::take(&mut self.initialized);
        let no_flow = self.flow;
        let no_reachable = no_flow.continues();
        let no_env = self.env.clone();
        if yes_reachable && !no_reachable {
            self.env = yes_env;
            self.initialized = yes_init;
        } else if !yes_reachable && no_reachable {
            self.env = no_env;
            self.initialized = no_init;
        } else if yes_reachable && no_reachable {
            yes.extend(self.application_join_releases(&yes_env, &no_env));
            no.extend(self.application_join_releases(&no_env, &yes_env));
            self.env = yes_env;
            self.join_initialization(&c, yes_init, no_init);
            self.join_environment(&no_env, Some(&c));
        } else {
            self.env = environment;
            self.join_initialization(&c, yes_init, no_init);
        }
        self.flow = yes_flow.union(no_flow);
        self.control_uniform = control && (c.uniform || (yes_reachable && no_reachable));
        self.code = previous;
        self.code.push(Inst::If {
            condition: c.regs[0],
            yes,
            no,
        });

        Ok(())
    }
    fn while_statement(&mut self, condition: &Expr, body: &[Stmt]) -> Result<()> {
        self.invalidate_assigned(body)?;
        self.invalidate_expression(condition)?;
        let environment = self.env.clone();
        let outer = std::mem::take(&mut self.code);
        let c = self.expr(condition)?;
        self.require_bool(&c, condition.span)?;
        let mut condition_code = std::mem::take(&mut self.code);
        if c.constant.as_ref().is_some_and(|bits| bits[0] == 0) {
            self.code = outer;
            self.code.extend(condition_code);
            return Ok(());
        }
        self.enter_loop(&environment);
        let condition_env = self.env.clone();
        let natural = if c.constant.as_ref().is_some_and(|bits| bits[0] != 0) {
            None
        } else {
            self.refine_bounds(&c, false);
            let exit = self.loop_state();
            self.env = condition_env;
            Some(exit)
        };
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.refine_bounds(&c, true);
        self.loop_depth += 1;
        self.iterators.push(None);
        self.block(body)?;
        let body_flow = self.flow;
        if body_flow.iterates() {
            self.check_application_loop(condition.span)?;
        }
        self.iterators.pop();
        self.loop_depth -= 1;
        let mut body = std::mem::take(&mut self.code);
        let next = self.leave_loop(natural, &mut condition_code, c.regs[0], &mut body);
        self.flow = body_flow.loop_result(next);
        self.control_uniform = control;
        self.code = outer;
        self.code.push(Inst::Loop {
            condition: condition_code,
            test: c.regs[0],
            body,
        });

        Ok(())
    }
    fn for_statement(
        &mut self,
        name: &String,
        start: &Expr,
        end: &Option<Expr>,
        body: &[Stmt],
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", "循环变量不能覆盖已有绑定");
        }
        let (begin, limit, data) = if let Some(end) = end {
            let a = self.expr(start)?;
            let b = self.expr(end)?;
            self.require_same(&Ty::scalar(Scalar::U32), &a.ty, start.span)?;
            self.require_same(&a.ty, &b.ty, start.span)?;
            (a, b, None)
        } else {
            let source = if let ExprKind::Name(data) = &start.kind {
                self.env.get(data).cloned()
            } else {
                None
            };
            let begin = self.constant(Scalar::U32, 0);
            match source {
                Some(Binding::ApplicationData(data)) => {
                    self.application_cpu(&data, start.span)?;
                    (
                        begin,
                        data.length.clone(),
                        Some(CursorSource::ApplicationData(data)),
                    )
                }
                Some(Binding::Tile(view)) => (
                    begin,
                    self.tile_length(&view),
                    Some(CursorSource::Tile(view)),
                ),
                Some(Binding::Data(view)) => {
                    let binding = view.binding;
                    if !matches!(self.view_access[&binding], Access::Read | Access::Atomic) {
                        return self.fail(
                            start.span,
                            "B128",
                            "顺序遍历需要 read 或 atomic 数据集；atomic 游标仍须显式原子访问",
                        );
                    }
                    (
                        begin,
                        self.data_length(&view),
                        Some(CursorSource::Data(binding)),
                    )
                }
                _ => {
                    let view = self.expr(start)?;
                    if view.ty.view.is_none() {
                        return self.fail(
                            start.span,
                            "B127",
                            "顺序遍历需要 dataset、tile 或不可变 view",
                        );
                    }
                    (
                        begin,
                        Self::view_length(&view),
                        Some(CursorSource::View(view)),
                    )
                }
            }
        };
        if begin.minimum() >= limit.maximum() {
            return Ok(());
        }
        let nonempty = begin.maximum() < limit.minimum();
        self.invalidate_assigned(body)?;
        let environment = self.env.clone();
        self.enter_loop(&environment);
        let slot = self.locals.len();
        self.locals.push(Scalar::U32);
        self.code.push(Inst::StoreLocal(slot, begin.regs[0]));
        let outer = std::mem::take(&mut self.code);
        let index = self.reg(Scalar::U32);
        self.code.push(Inst::LoadLocal(index, slot));
        let cursor = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![index],
            uniform: begin.uniform && limit.uniform,
            constant: None,
        };
        let test = self.binary("<", cursor.clone(), limit.clone(), span)?;
        let mut condition = std::mem::take(&mut self.code);
        self.env.insert(
            name.clone(),
            if let Some(domain) = data {
                match domain {
                    CursorSource::ApplicationData(data) => Binding::Owned(OwnedBinding {
                        place: Self::application_element(
                            &data,
                            index,
                            (0, data.length.maximum().saturating_sub(1)),
                        ),
                        proof: bounds::Proof::default(),
                        uniform: false,
                        initialization: None,
                    }),
                    CursorSource::Data(binding) => Binding::Element {
                        binding,
                        index,
                        writable: false,
                    },
                    CursorSource::Tile(view) => Binding::TileElement { view, index },
                    CursorSource::View(view) => Binding::ViewElement { view, index },
                }
            } else {
                Binding::Value(Value {
                    proof: bounds::for_cursor(&begin, &limit, body),
                    ..cursor
                })
            },
        );
        let condition_env = self.env.clone();
        self.refine_bounds(&test, false);
        let natural = self.loop_state();
        self.env = condition_env;
        self.refine_bounds(&test, true);
        let control = self.control_uniform;
        self.control_uniform &= begin.uniform && limit.uniform;
        self.loop_depth += 1;
        self.iterators.push(Some((name.clone(), slot)));
        self.block(body)?;
        let body_flow = self.flow;
        if body_flow.iterates() {
            self.check_application_loop(span)?;
        }
        let body_reachable = body_flow.continues();
        self.iterators.pop();
        self.loop_depth -= 1;
        if body_reachable {
            self.advance_iterator(index, slot);
        }
        let mut body = std::mem::take(&mut self.code);
        let natural = (!nonempty || body_flow.iterates()).then_some(natural);
        let next = self.leave_loop(natural, &mut condition, test.regs[0], &mut body);
        self.flow = body_flow.loop_result(next);
        self.control_uniform = control;
        self.code = outer;
        self.code.push(Inst::Loop {
            condition,
            test: test.regs[0],
            body,
        });

        Ok(())
    }
    fn let_statement(
        &mut self,
        annotation: &Option<TypeName>,
        name: &String,
        e: &Expr,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if annotation.as_ref().is_some_and(|ty| ty.dataset) || self.application_data_expression(e) {
            return self.application_dataset(annotation, name, e, span);
        }
        if let ExprKind::Call(call, args) = &e.kind {
            if matches!(call.as_str(), "tile_publish" | "tile_atomic") {
                return self.tile_publish(
                    annotation.is_some(),
                    name,
                    args,
                    span,
                    call == "tile_atomic",
                );
            }
        }
        let value = if let ExprKind::Cooperate(call, explicit, args) = &e.kind {
            let result = self.cooperate(call, explicit.as_deref(), args, e.span)?;
            self.value_result(result, e.span)?
        } else if let ExprKind::Call(call, args) = &e.kind {
            if let Some(op) = TileOp::from_source(call) {
                self.tile_collective(op, args, span)?
            } else {
                self.expr(e)?
            }
        } else {
            self.expr(e)?
        };
        if let Some(ty) = annotation {
            let ty = self.world.ty(
                ty,
                &self.definition.source,
                span,
                &self.definition.module,
                &self.definition.imports,
            )?;
            self.require_same(&ty, &value.ty, span)?;
        }
        if !self.mutable_names.contains(name) && value.ty.contains_arrays() {
            let proof = bounds::Proof::capture(&value);
            let uniform = value.uniform && self.control_uniform;
            let place = self.materialize_array(value);
            self.env.insert(
                name.clone(),
                Binding::Owned(OwnedBinding {
                    place,
                    proof,
                    uniform,
                    initialization: None,
                }),
            );
            return Ok(());
        }
        if !self.mutable_names.contains(name) {
            let mut value = value;
            value.literal = false;
            value.uniform &= self.control_uniform;
            self.env.insert(name.clone(), Binding::Value(value));
            return Ok(());
        }
        let (slots, storage) = self.local_slots(&value.ty);
        for (slot, r) in slots.iter().zip(&value.regs) {
            self.code.push(Inst::StoreLocal(*slot, *r));
        }
        let proof = bounds::Proof::capture(&value);
        let place = Place::Local(value.ty, slots, storage);
        self.initialize(&place);
        self.env.insert(
            name.clone(),
            Binding::Owned(OwnedBinding {
                proof,
                place,
                uniform: value.uniform && self.control_uniform,
                initialization: None,
            }),
        );

        Ok(())
    }
    fn numeric_constructor(&mut self, ty: Ty, mut values: Vec<Value>, span: Span) -> Result<Value> {
        if ty.matrix_shape().is_some() {
            return self.matrix_construct(ty, values, span);
        }
        if values.iter().any(|v| v.ty.array.is_some()) {
            return self.fail(span, "B238", "数组不能转换成数值标量／向量；先取得数组元素");
        }
        if values.iter().any(|v| v.ty.matrix_shape().is_some()) {
            return self.fail(span, "B237", "矩阵不能转换成数值标量／向量；先取得行字段");
        }
        if values.iter().any(|v| v.ty.contains_views()) {
            return self.fail(span, "B185", "view 不能转换成数值或重解释设备地址");
        }
        if values.len() == 1 && values[0].ty == ty {
            let mut value = values.pop().unwrap();
            value.literal = false;
            return Ok(value);
        }
        let mut flat = Vec::new();
        let mut constant = Some(Vec::new());
        let mut proof = bounds::Proof::default();
        let uniform = values.iter().all(|v| v.uniform);
        for v in values {
            let v = self.cast(v, ty.components[0].0);
            if ty.components.len() == 1 {
                proof = v.proof.clone();
            } else {
                proof.append(&v);
            }
            constant = constant.zip(v.constant).map(|(mut all, part)| {
                all.extend(part);
                all
            });
            flat.extend(v.regs);
        }
        if flat.len() == 1 && ty.components.len() > 1 {
            proof.splat(ty.components.len());
            flat = vec![flat[0]; ty.components.len()];
            constant = constant.map(|v| vec![v[0]; ty.components.len()]);
        }
        if flat.len() != ty.components.len() {
            return self.fail(span, "B157", "类型构造的分量数不匹配");
        }
        return Ok(Value {
            proof,
            literal: false,
            ty,
            regs: flat,
            uniform,
            constant,
        });
    }
    fn dot_value(&mut self, values: Vec<Value>, span: Span) -> Result<Value> {
        if values.len() != 2 {
            return self.fail(span, "B153", "dot 需要两个向量");
        }
        self.require_same(&values[0].ty, &values[1].ty, span)?;
        if values[0].ty.components[0].0 != Scalar::F32 || !values[0].ty.is_numeric() {
            return self.fail(span, "B154", "dot 需要 float 向量");
        }
        let p = self.binary("*", values[0].clone(), values[1].clone(), span)?;
        let mut r = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::F32),
            regs: vec![p.regs[0]],
            uniform: p.uniform,
            constant: None,
        };
        for x in p.regs.iter().skip(1) {
            r = self.binary(
                "+",
                r,
                Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: Ty::scalar(Scalar::F32),
                    regs: vec![*x],
                    uniform: p.uniform,
                    constant: None,
                },
                span,
            )?;
        }
        return Ok(r);
    }
    fn length_value(
        &mut self,
        mut values: Vec<Value>,
        normalize: bool,
        span: Span,
    ) -> Result<Value> {
        if values.len() != 1 {
            return self.fail(span, "B153", "length／normalize 需要一个向量");
        }
        let v = values.remove(0);
        if v.ty.components[0].0 != Scalar::F32 || !v.ty.fields.is_empty() || v.ty.array.is_some() {
            return self.fail(span, "B154", "需要 float 向量");
        }
        let p = self.binary("*", v.clone(), v.clone(), span)?;
        let mut r = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::F32),
            regs: vec![p.regs[0]],
            uniform: p.uniform,
            constant: None,
        };
        for x in p.regs.iter().skip(1) {
            r = self.binary(
                "+",
                r,
                Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: Ty::scalar(Scalar::F32),
                    regs: vec![*x],
                    uniform: p.uniform,
                    constant: None,
                },
                span,
            )?;
        }
        let length = self.math("sqrt", vec![r], span)?;
        if !normalize {
            Ok(length)
        } else {
            self.binary("/", v, length, span)
        }
    }
    fn cross_value(&mut self, values: Vec<Value>, span: Span) -> Result<Value> {
        if values.len() != 2 {
            return self.fail(span, "B153", "cross 需要两个 float3");
        }
        for v in &values {
            self.require_same(&Ty::vector(Scalar::F32, 3), &v.ty, span)?;
        }
        let a = &values[0];
        let b = &values[1];
        let mut regs = Vec::new();
        for (i, j) in [(1, 2), (2, 0), (0, 1)] {
            let value = |r, u| Value {
                proof: bounds::Proof::default(),
                literal: false,
                ty: Ty::scalar(Scalar::F32),
                regs: vec![r],
                uniform: u,
                constant: None,
            };
            let x = self.binary(
                "*",
                value(a.regs[i], a.uniform),
                value(b.regs[j], b.uniform),
                span,
            )?;
            let y = self.binary(
                "*",
                value(a.regs[j], a.uniform),
                value(b.regs[i], b.uniform),
                span,
            )?;
            regs.push(self.binary("-", x, y, span)?.regs[0]);
        }
        return Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::vector(Scalar::F32, 3),
            regs,
            uniform: a.uniform && b.uniform,
            constant: None,
        });
    }
    fn member_place(&mut self, base: &Expr, field: &String, e: &Expr) -> Result<Place> {
        let p = self.place(base)?;
        if p.ty().view.is_some() {
            return self.fail(e.span, "B185", "view 元数据不可修改或按分量访问");
        }
        let (sub, components) = self.member(p.ty(), field, e.span)?;
        Ok(p.project(sub, &components))
    }
    fn index_place(&mut self, base: &Expr, index: &Expr) -> Result<Place> {
        let ExprKind::Name(name) = &base.kind else {
            return self.view_place(base, index);
        };
        if let Some(Binding::ApplicationData(data)) = self.env.get(name).cloned() {
            return self.application_index(data, index);
        }
        if let Some(Binding::Tile(view)) = self.env.get(name).cloned() {
            return self.tile_place(view, index);
        }
        let Some(Binding::Data(view)) = self.env.get(name).cloned() else {
            return self.view_place(base, index);
        };
        let binding = view.binding;
        let i = self.expr(index)?;
        self.require_same(&Ty::scalar(Scalar::U32), &i.ty, index.span)?;
        let current = i.regs[0] == self.global_index;
        let same_shape = self.program.buffers[binding].count.is_some()
            && self.program.buffers[binding].count == self.program.buffers[self.domain].count;
        if !self.index_in(
            &i,
            IndexDomain::Data(binding),
            self.program.buffers[binding].count.unwrap_or(0),
        ) && !(current && (binding == self.domain || same_shape))
        {
            return self.fail(
                index.span,
                "B137",
                "索引有效性无法静态证明；使用有界游标、同域 index()、有效长度条件或形状约束",
            );
        }
        if self.view_access[&binding] != Access::Atomic
            && self.view_access[&binding].writes()
            && !current
        {
            return self.fail(
                index.span,
                "B166",
                "并行可写数据只允许访问当前独占元素；读取其他元素需要独立 read 数据集",
            );
        }
        if matches!(self.view_access[&binding], Access::Write | Access::InOut) {
            if self
                .appends
                .iter()
                .any(|(output, count)| *output == binding || count.binding == binding)
            {
                return self.fail(
                    index.span,
                    "B221",
                    "append 的共享输出／计数不能在同一阶段借为普通独占元素",
                );
            }
            self.element_domains.insert(binding);
        }
        let ty = self.program.buffers[binding].ty.clone();
        let words = ty.components.iter().map(|x| x.1 / 4).collect();
        Ok(Place::Element {
            ty,
            binding,
            index: i.regs[0],
            words,
            writable: self.view_access[&binding].writes() && current,
        })
    }
    fn name_place(&mut self, name: &String, e: &Expr) -> Result<Place> {
        match self.env.get(name).cloned() {
            Some(Binding::AtomicBorrow(place)) => Ok(place),
            Some(Binding::Owned(owner)) => Ok(owner.place),
            Some(Binding::Value(value)) if value.ty.contains_arrays() => {
                let place = self.materialize_array(value);
                self.env.insert(
                    name.clone(),
                    Binding::Owned(OwnedBinding {
                        place: place.clone(),
                        proof: bounds::Proof::default(),
                        uniform: false,
                        initialization: None,
                    }),
                );
                Ok(place)
            }
            Some(Binding::Element {
                binding,
                index,
                writable,
            }) => {
                let ty = self.program.buffers[binding].ty.clone();
                let words = ty.components.iter().map(|x| x.1 / 4).collect();
                Ok(Place::Element {
                    ty,
                    binding,
                    index,
                    words,
                    writable,
                })
            }
            Some(Binding::TileElement { view, index }) => Ok(Place::TileElement {
                ty: view.ty,
                tile: view.tile,
                index,
                words: view.words,
            }),
            Some(Binding::ViewElement { view, index }) => Ok(Self::view_element(&view, index)),
            _ => self.fail(e.span, "B134", format!("{name} 不是可赋值的元素或局部变量")),
        }
    }
    fn declare_statement(
        &mut self,
        annotation: &TypeName,
        name: &String,
        span: Span,
    ) -> Result<()> {
        if annotation.dataset {
            return self.fail(
                span,
                "B263",
                "动态 dataset 声明需要 dataset<T>(length) 创建存储",
            );
        }
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        let ty = self.world.ty(
            annotation,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        let (slots, storage) = self.local_slots(&ty);
        self.env.insert(
            name.clone(),
            Binding::Owned(OwnedBinding {
                place: Place::Local(ty, slots, storage),
                proof: bounds::Proof::default(),
                uniform: false,
                initialization: None,
            }),
        );

        Ok(())
    }
    fn assign_statement(&mut self, lhs: &Expr, op: &String, rhs: &Expr, span: Span) -> Result<()> {
        let place = self.place(lhs)?;
        let mut value = self.expr(rhs)?;
        if op != "=" {
            let current = self.read_place(&place, lhs.span)?;
            value = self.binary(&op[..1], current, value, span)?;
        }
        self.write_place(&place, value, span)?;

        Ok(())
    }
    fn expression_statement(&mut self, e: &Expr) -> Result<()> {
        if self.application_data_expression(e) {
            let data = self.application_resource(e, e.span)?;
            self.release_application_handle(data.handle);
            return Ok(());
        }
        if let ExprKind::Call(name, args) = &e.kind
            && name == "atomic_store"
        {
            self.atomic_store(args, e.span)?;
        } else if let ExprKind::Call(name, args) = &e.kind
            && name == "tile_barrier"
        {
            self.tile_barrier(args, e.span)?;
        } else if let ExprKind::Call(name, args) = &e.kind {
            self.call(name, args, e.span)?;
        } else if let ExprKind::Specialize(name, explicit, args) = &e.kind {
            self.specialized_call(name, explicit, args, e.span)?;
        } else if let ExprKind::Cooperate(name, explicit, args) = &e.kind {
            self.cooperate(name, explicit.as_deref(), args, e.span)?;
        } else {
            self.expr(e)?;
        }

        Ok(())
    }
    fn advance_iterator(&mut self, index: Reg, slot: usize) {
        let one = self.constant(Scalar::U32, 1);
        let next = self.reg(Scalar::U32);
        self.code.push(Inst::Binary {
            dst: next,
            op: BinaryOp::IAdd,
            a: index,
            b: one.regs[0],
        });
        self.code.push(Inst::StoreLocal(slot, next));
    }
    fn seek_statement(&mut self, name: &String, position: &Expr, span: Span) -> Result<()> {
        let Some(Some((iterator, slot))) = self.iterators.last().cloned() else {
            return self.fail(span, "B164", "seek 需要当前有界 for 游标");
        };
        if iterator != *name {
            return self.fail(span, "B165", "seek 只能推进当前最内层游标");
        }
        let value = self.expr(position)?;
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, position.span)?;
        self.check_application_loop(span)?;
        self.release_application_loop();
        self.code.push(Inst::StoreLocal(slot, value.regs[0]));
        self.code.push(Inst::Continue);
        self.flow = Flow::CONTINUE;

        Ok(())
    }
    fn ray_query_call(&mut self, name: &str, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 5 {
            return self.fail(
                span,
                "B183",
                "光线查询需要 acceleration、origin、minimum、direction、maximum",
            );
        }
        let ExprKind::Name(scene) = &args[0].kind else {
            return self.fail(span, "B183", "光线查询需要命名 acceleration 资源");
        };
        let Some(Binding::Acceleration(scene)) = self.env.get(scene).cloned() else {
            return self.fail(span, "B183", "光线查询需要 read acceleration 资源");
        };
        let origin = self.expr(&args[1])?;
        let minimum = self.expr(&args[2])?;
        let direction = self.expr(&args[3])?;
        let maximum = self.expr(&args[4])?;
        self.require_same(&Ty::vector(Scalar::F32, 3), &origin.ty, args[1].span)?;
        self.require_same(&Ty::scalar(Scalar::F32), &minimum.ty, args[2].span)?;
        self.require_same(&origin.ty, &direction.ty, args[3].span)?;
        self.require_same(&minimum.ty, &maximum.ty, args[4].span)?;
        let ty = if name == "trace_any" {
            Ty::scalar(Scalar::Bool)
        } else {
            Ty::builtin("RayHit").unwrap()
        };
        let regs: Vec<_> = ty.components.iter().map(|(s, _)| self.reg(*s)).collect();
        let result = if name == "trace_any" {
            RayResult::Any(regs[0])
        } else {
            RayResult::Closest(regs.as_slice().try_into().unwrap())
        };
        self.code.push(Inst::RayQuery {
            scene,
            origin: origin.regs.as_slice().try_into().unwrap(),
            direction: direction.regs.as_slice().try_into().unwrap(),
            minimum: minimum.regs[0],
            maximum: maximum.regs[0],
            result,
        });
        return Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: false,
            constant: None,
        }));
    }
    fn unsafe_read_call(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 2 {
            return self.fail(
                span,
                "B181",
                "unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性",
            );
        }
        if let ExprKind::Name(data) = &args[0].kind {
            if let Some(Binding::ApplicationData(data)) = self.env.get(data).cloned() {
                self.application_cpu(&data, span)?;
                let index = self.expr(&args[1])?;
                self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args[1].span)?;
                return self
                    .read_place(
                        &Self::application_element(
                            &data,
                            index.regs[0],
                            (index.minimum(), index.maximum()),
                        ),
                        span,
                    )
                    .map(Some);
            }
            if let Some(Binding::Data(view)) = self.env.get(data).cloned() {
                let binding = view.binding;
                if self.view_access[&binding] != Access::Read {
                    return self.fail(
                        span,
                        "B181",
                        "unsafe_read 只允许不可变数据集，不能别名读取并行写入的数据",
                    );
                }
                let index = self.expr(&args[1])?;
                self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args[1].span)?;
                let ty = self.program.buffers[binding].ty.clone();
                let words = ty.components.iter().map(|x| x.1 / 4).collect();
                return self
                    .read_place(
                        &Place::Element {
                            ty,
                            binding,
                            index: index.regs[0],
                            words,
                            writable: false,
                        },
                        span,
                    )
                    .map(Some);
            }
        }
        let view = self.expr(&args[0])?;
        if view.ty.view.is_none() {
            return self.fail(span, "B181", "unsafe_read 需要 read dataset 或不可变 view");
        }
        let index = self.expr(&args[1])?;
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args[1].span)?;
        return self
            .read_place(&Self::view_element(&view, index.regs[0]), span)
            .map(Some);
    }
    fn ordinal_call(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 1 {
            return self.fail(span, "B168", "ordinal 需要一个有界游标");
        }
        let ExprKind::Name(name) = &args[0].kind else {
            return self.fail(span, "B168", "ordinal 需要命名游标");
        };
        let (domain, index, proof) = match self.env.get(name).cloned() {
            Some(Binding::Owned(OwnedBinding {
                place: Place::ApplicationElement { handle, index, .. },
                ..
            })) => (
                IndexDomain::ApplicationData(handle),
                index,
                bounds::Proof::unsigned(0, u32::MAX),
            ),
            Some(Binding::Owned(OwnedBinding {
                place: Place::Element { binding, index, .. },
                ..
            })) => (
                IndexDomain::Data(binding),
                index,
                self.data_index_proof(binding),
            ),
            Some(Binding::Element { binding, index, .. }) => (
                IndexDomain::Data(binding),
                index,
                self.data_index_proof(binding),
            ),
            Some(Binding::TileElement { index, .. }) => {
                (IndexDomain::Tile, index, self.tile_index_proof())
            }
            Some(Binding::ViewElement { view, index }) => {
                let proof = bounds::Proof::unsigned(
                    0,
                    Self::view_length(&view).maximum().saturating_sub(1),
                );
                (IndexDomain::View(view.view_identity()), index, proof)
            }
            _ => return self.fail(span, "B168", "ordinal 只能用于元素游标"),
        };
        return Ok(Some(Value {
            proof: proof.with_domains(Domains::from([domain])),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![index],
            uniform: false,
            constant: None,
        }));
    }
    fn member_expr(&mut self, base: &Expr, field: &String, e: &Expr) -> Result<Value> {
        if let ExprKind::Name(name) = &base.kind {
            if let Some(Binding::ApplicationData(data)) = self.env.get(name).cloned() {
                self.application_available(&data, e.span)?;
                return if field == "length" {
                    Ok(data.length)
                } else {
                    self.fail(e.span, "B141", "dataset 只公开只读 length")
                };
            }
            if let Some(Binding::Texture(reference)) = self.env.get(name).cloned() {
                if field == "length"
                    && let Some(count) = reference.shape(self.program)
                {
                    return Ok(self.constant(Scalar::U32, count));
                }
                return self.fail(e.span, "B239", "纹理资源数组只公开只读 length");
            }
            if let Some(Binding::Tile(view)) = self.env.get(name).cloned() {
                return match field.as_str() {
                    "length" => Ok(self.tile_length(&view)),
                    "lane" => Ok(self.tile_lane()),
                    _ => self.fail(e.span, "B141", "tile 只公开只读 length 与 lane"),
                };
            }
            if let Some(Binding::Data(view)) = self.env.get(name).cloned() {
                if field == "length" {
                    return Ok(self.data_length(&view));
                }
            }
        }
        if self.is_place(base) {
            // 保留字段访问，避免为了读取一个字段先加载整个结构体。
            let p = self.place(base)?;
            if p.ty().array.is_some() && field == "length" {
                return Ok(self.constant(Scalar::U32, p.ty().values[0]));
            }
            if p.ty().view.is_some() {
                if field == "length" {
                    let view = self.read_place(&p, e.span)?;
                    return Ok(Self::view_length(&view));
                }
                return self.fail(e.span, "B185", "view 只公开只读 length，不公开设备地址分量");
            }
            let (sub, components) = self.member(p.ty(), field, e.span)?;
            return self.read_place(&p.project(sub, &components), e.span);
        }
        let v = self.expr(base)?;
        if v.ty.array.is_some() && field == "length" {
            return Ok(self.constant(Scalar::U32, v.ty.values[0]));
        }
        if v.ty.view.is_some() && field == "length" {
            return Ok(Self::view_length(&v));
        }
        let (ty, indices) = self.member(&v.ty, field, e.span)?;
        Ok(Value {
            proof: v.projected_proof(&indices),
            literal: false,
            ty,
            regs: indices.iter().map(|i| v.regs[*i]).collect(),
            uniform: v.uniform,
            constant: v.constant.map(|b| indices.iter().map(|i| b[*i]).collect()),
        })
    }
    fn index_expr(&mut self, base: &Expr, index: &Expr, e: &Expr) -> Result<Value> {
        let resource = matches!(&base.kind, ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::Data(_) | Binding::Tile(_) | Binding::ApplicationData(_))));
        if !resource && !self.is_place(base) {
            let value = self.expr(base)?;
            if value.ty.array.is_some() {
                return self.array_value_index(value, index);
            }
            let place = self.view_value_place(value, index)?;
            return self.read_place(&place, e.span);
        }
        let p = self.place(e)?;
        self.read_place(&p, e.span)
    }
    fn name_expr(&mut self, name: &String, e: &Expr) -> Result<Value> {
        match self.env.get(name).cloned() {
            Some(Binding::Value(v)) => Ok(v),
            Some(Binding::Owned(owner)) => self.read_place(&owner.place, e.span),
            Some(
                Binding::AtomicBorrow(_)
                | Binding::Element { .. }
                | Binding::TileElement { .. }
                | Binding::ViewElement { .. },
            ) => {
                let p = self.place(e)?;
                self.read_place(&p, e.span)
            }
            None => {
                let key = self
                    .world
                    .resolve(
                        name,
                        &self.definition.module,
                        &self.definition.imports,
                        true,
                    )
                    .filter(|key| self.world.functions[key].function.kind == FunctionKind::Constant)
                    .ok_or_else(|| Diagnostic {
                        source: self.definition.source.clone(),
                        span: e.span,
                        code: "B145",
                        message: format!("未知值 {name}"),
                    })?;
                self.module_constant(key)
            }
            _ => self.fail(e.span, "B145", format!("未知值 {name}")),
        }
    }
    fn is_place(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Name(n) => {
                matches!(
                    self.env.get(n),
                    Some(
                        Binding::AtomicBorrow(_)
                            | Binding::Element { .. }
                            | Binding::TileElement { .. }
                            | Binding::ViewElement { .. }
                            | Binding::Owned(_)
                    )
                )
            }
            ExprKind::Member(b, _) => self.is_place(b),
            ExprKind::Index(..) => true,
            _ => false,
        }
    }
}
use effects::{assigned_names, expression_assignments};
