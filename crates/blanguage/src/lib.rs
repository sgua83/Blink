//! BLanguage .h/.b 前端、类型和访问检查、并行 IR 与原生代码生成。
pub mod application;
pub mod ast;
mod check;
pub mod dll;
mod generics;
pub mod ir;
mod lexer;
pub mod native;
mod optimize;
mod parser;
pub mod spirv;
mod type_sets;
mod value_parameters;

use ast::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub(crate) struct Definition {
    function: Function,
    source: Source,
    module: String,
    imports: Vec<String>,
    public: bool,
}
/// 仅由已完成模块／阶段证明的无前置条件程序构造；执行器不接受条件模板。
#[derive(Clone, Debug)]
pub struct ExecutableProgram(ir::Program);
impl ExecutableProgram {
    /// 恢复 blc 生成的已检查编译结果；供独立 DLL 内嵌程序使用。
    ///
    /// # Safety
    /// 参数必须是本版本编译器对同一无前置条件入口生成的完整、未修改的结果。
    /// 不得由宿主输入构造，不能把条件模板伪装成可执行程序。
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn from_compiler_output(
        name: String,
        target: Target,
        buffers: Vec<ir::Buffer>,
        accelerations: Vec<String>,
        textures: Vec<ir::TextureBinding>,
        uniforms: Vec<ir::Uniform>,
        uniform_bytes: u32,
        source_stages: usize,
        stages: Vec<ir::Stage>,
        prefix_requirements: Vec<ir::PrefixRelation>,
        prefix_guarantees: Vec<ir::PrefixRelation>,
    ) -> Self {
        Self(ir::Program {
            name,
            target,
            buffers,
            accelerations,
            textures,
            uniforms,
            uniform_bytes,
            source_stages,
            stages,
            prefix_requirements,
            prefix_guarantees,
        })
    }
}
impl std::ops::Deref for ExecutableProgram {
    type Target = ir::Program;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// 接口条件已检查的程序；只能以执行结果持有的域关系绑定，不能作为裸数据入口。
#[derive(Clone, Debug)]
pub struct TemplateProgram(ir::Program);
impl std::ops::Deref for TemplateProgram {
    type Target = ir::Program;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
pub struct Compilation {
    pub programs: Vec<ExecutableProgram>,
    pub templates: Vec<TemplateProgram>,
    pub open_generics: Vec<String>,
    pub applications: Vec<application::Application>,
}
pub(crate) struct World {
    sources: BTreeMap<String, Source>,
    records: BTreeMap<String, (Record, Source, String, Vec<String>)>,
    functions: BTreeMap<String, Arc<Definition>>,
    type_sets: BTreeMap<String, (TypeSet, Source, String, Vec<String>)>,
    evaluating_constants: RefCell<BTreeSet<String>>,
}
fn read(path: &Path) -> Result<Source> {
    let text = std::fs::read_to_string(path).map_err(|e| Diagnostic {
        source: Source {
            path: path.into(),
            text: "".into(),
        },
        span: Span::default(),
        code: "B020",
        message: format!("无法读取模块：{e}"),
    })?;
    Ok(Source {
        path: path.into(),
        text: text.into(),
    })
}
fn contract_signature(f: &Function) -> BTreeSet<(ContractKind, &ContractPredicate)> {
    f.contracts.iter().map(|c| (c.kind, &c.predicate)).collect()
}
fn same_signature(a: &Function, b: &Function) -> bool {
    a.kind == b.kind
        && a.external == b.external
        && a.types == b.types
        && a.values == b.values
        && a.result == b.result
        && a.target == b.target
        && contract_signature(a) == contract_signature(b)
        && a.params.len() == b.params.len()
        && a.params
            .iter()
            .zip(&b.params)
            .all(|(a, b)| a.name == b.name && a.ty == b.ty && a.access == b.access)
}
pub fn compile(path: &Path, module_root: &Path) -> Result<Compilation> {
    compile_module(path, module_root, true, None)
}
/// 选择公开 program 的具体静态入口，例如 shade<13> 或 run<float3,17>。
pub fn compile_entry(path: &Path, module_root: &Path, entry: &str) -> Result<Compilation> {
    compile_module(
        path,
        module_root,
        true,
        Some((FunctionKind::Program, entry)),
    )
}
/// 编译独立 CPU 应用的普通值入口；启动与平台链接由 application 模块拥有。
pub fn compile_application(
    path: &Path,
    module_root: &Path,
    entry: &str,
) -> Result<application::Application> {
    Ok(
        compile_module(path, module_root, false, Some((FunctionKind::Value, entry)))?
            .applications
            .remove(0),
    )
}
/// 库接口与完整导入闭包同样检查；无需为函数库伪造可执行 program。
pub fn check(path: &Path, module_root: &Path) -> Result<Compilation> {
    compile_module(path, module_root, false, None)
}
fn compile_module(
    path: &Path,
    module_root: &Path,
    require_program: bool,
    requested: Option<(FunctionKind, &str)>,
) -> Result<Compilation> {
    let mut world = World {
        sources: BTreeMap::new(),
        records: BTreeMap::new(),
        functions: BTreeMap::new(),
        type_sets: BTreeMap::new(),
        evaluating_constants: RefCell::new(BTreeSet::new()),
    };
    let mut loaded = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    let entry = load(path, module_root, &mut world, &mut loaded, &mut visiting)?;
    let selected = if let Some((kind, request)) = requested {
        let source = Source {
            path: PathBuf::from("<entry>"),
            text: request.into(),
        };
        let (name, arguments) = parser::entry(source.clone())?;
        let key = world
            .resolve(&name, &entry, &[], true)
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span: Span::default(),
                code: "B240",
                message: format!("没有公开入口 {name}"),
            })?;
        let definition = &world.functions[&key];
        if !definition.public || definition.module != entry || definition.function.kind != kind {
            return source.fail(
                Span::default(),
                "B240",
                "入口选择需要本模块对应种类的公开函数",
            );
        }
        Some((key, arguments, kind))
    } else {
        None
    };
    for name in world.type_sets.keys() {
        world.set_members(name, &mut BTreeSet::new())?;
    }
    for (name, (record, source, module, imports)) in &world.records {
        generics::validate_parameters(&record.types, &record.name, source, record.span)?;
        value_parameters::validate(&record.values, &record.name, &[], source, record.span)?;
        let values = value_parameters::combinations(&record.values);
        for arguments in
            generics::combinations(&world, &record.types, source, record.span, module, imports)?
        {
            for values in &values {
                let mut ty = TypeName::named(name);
                ty.arguments = arguments
                    .iter()
                    .cloned()
                    .map(GenericArgument::Type)
                    .chain(
                        values
                            .iter()
                            .map(|value| GenericArgument::Value(Shape::Literal(*value))),
                    )
                    .collect();
                world.ty(&ty, source, record.span, module, imports)?;
            }
        }
    }

    let mut programs = Vec::new();
    let mut templates = Vec::new();
    let mut applications = Vec::new();
    // 有限声明逐项检查；选定的入口与开放 const 声明在具体使用处实例化。
    for (base, definition) in &world.functions {
        generics::validate_declaration(definition)?;
        if selected.as_ref().is_some_and(|(key, _, _)| key == base) {
            continue;
        }
        for instance in generics::instances(&world, base, definition)? {
            let name = &instance.name;
            let d = &instance.definition;
            if d.function.kind == FunctionKind::Program {
                let p = check::compile_program(&world, &instance)
                    .map_err(|error| instance_error(error, name))?;
                if selected.is_none() && d.module == entry && d.public {
                    if p.prefix_requirements.is_empty() {
                        programs.push(ExecutableProgram(p));
                    } else {
                        templates.push(TemplateProgram(p));
                    }
                }
            } else {
                check::check_function(&world, &instance)
                    .map_err(|error| instance_error(error, name))?;
            }
        }
    }
    if let Some((key, arguments, kind)) = selected {
        let definition = &world.functions[&key];
        let instance = check::generics::specialize(
            &world,
            &key,
            &[],
            arguments.as_deref(),
            definition,
            definition.function.span,
        )?;
        if kind == FunctionKind::Value {
            applications.push(check::compile_application(&world, &instance)?);
        } else {
            let p = check::compile_program(&world, &instance)
                .map_err(|error| instance_error(error, &instance.name))?;
            if p.prefix_requirements.is_empty() {
                programs.push(ExecutableProgram(p));
            } else {
                templates.push(TemplateProgram(p));
            }
        }
    }
    let open_generics: Vec<_> = world
        .records
        .iter()
        .filter(|(_, (record, ..))| record.values.iter().any(|p| p.choices.is_none()))
        .map(|(name, _)| name.clone())
        .chain(
            world
                .functions
                .iter()
                .filter(|(_, d)| d.function.values.iter().any(|p| p.choices.is_none()))
                .map(|(name, _)| name.clone()),
        )
        .collect();
    if require_program && programs.is_empty() && templates.is_empty() {
        if world.functions.values().any(|d| {
            d.module == entry
                && d.public
                && d.function.kind == FunctionKind::Program
                && d.function.values.iter().any(|p| p.choices.is_none())
        }) {
            return world.sources[&entry].fail(
                Span::default(),
                "B240",
                "开放 const program 需要选择具体入口，例如 compile_entry(..., \"run<13>\")",
            );
        }
        return world.sources[&entry].fail(
            Span::default(),
            "B021",
            "入口模块没有公开的 program；函数库使用 check，并在 program 中实例化原生代码",
        );
    }
    Ok(Compilation {
        programs,
        templates,
        open_generics,
        applications,
    })
}
fn instance_error(mut error: Diagnostic, name: &str) -> Diagnostic {
    if name.contains('<') {
        error.message += &format!("（检查泛型实例 {name}）");
    }
    error
}
fn load(
    path: &Path,
    root: &Path,
    world: &mut World,
    loaded: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<String> {
    let implementation = parser::parse(read(&path.with_extension("b"))?)?;
    let interface = parser::parse(read(&path.with_extension("h"))?)?;
    if implementation.name != interface.name {
        return implementation
            .source
            .fail(Span::default(), "B022", ".h 与 .b 的模块名不一致");
    }
    let name = implementation.name.clone();
    if loaded.contains(&name) {
        return Ok(name);
    }
    world
        .sources
        .insert(name.clone(), implementation.source.clone());
    if !visiting.insert(name.clone()) {
        return implementation.source.fail(
            Span::default(),
            "B023",
            "模块导入存在环；公共类型应移入共同依赖模块",
        );
    }
    let imports: Vec<_> = interface
        .imports
        .iter()
        .chain(&implementation.imports)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for import in &imports {
        let import_path: PathBuf = root.join(import.replace('.', "/")).with_extension("b");
        let actual = load(&import_path, root, world, loaded, visiting)?;
        if actual != *import {
            return implementation.source.fail(
                Span::default(),
                "B024",
                format!("导入 {import}，文件却声明 {actual}"),
            );
        }
    }
    for (record, source) in interface
        .records
        .iter()
        .map(|r| (r, &interface.source))
        .chain(
            implementation
                .records
                .iter()
                .map(|r| (r, &implementation.source)),
        )
    {
        if matches!(
            record.name.as_str(),
            "acceleration" | "dataset" | "view" | "tile" | "array" | "texture2d" | "void"
        ) || ir::Ty::builtin(&record.name).is_some()
        {
            return source.fail(record.span, "B184", "记录名称不能覆盖内置类型");
        }
        let key = format!("{name}::{}", record.name);
        if world.records.contains_key(&key) {
            return source.fail(record.span, "B025", format!("重复类型 {key}"));
        }
        world.records.insert(
            key,
            (
                record.clone(),
                source.clone(),
                name.clone(),
                imports.clone(),
            ),
        );
    }
    for (set, source) in interface
        .type_sets
        .iter()
        .map(|s| (s, &interface.source))
        .chain(
            implementation
                .type_sets
                .iter()
                .map(|s| (s, &implementation.source)),
        )
    {
        let key = format!("{name}::{}", set.name);
        if world.records.contains_key(&key)
            || world.type_sets.contains_key(&key)
            || ir::Ty::builtin(&set.name).is_some()
            || matches!(
                set.name.as_str(),
                "void" | "dataset" | "view" | "tile" | "array" | "acceleration" | "texture2d"
            )
            || matches!(
                set.name.as_str(),
                "numeric" | "floating" | "integer" | "scalar_numeric" | "scalar_integer"
            )
        {
            return source.fail(
                set.span,
                "B233",
                format!("类型集合名称重复或覆盖已有类型 {key}"),
            );
        }
        world.type_sets.insert(
            key,
            (set.clone(), source.clone(), name.clone(), imports.clone()),
        );
    }
    for f in interface
        .functions
        .iter()
        .filter(|f| f.kind != FunctionKind::Constant && f.external.is_none())
    {
        if f.body.is_some() {
            return interface
                .source
                .fail(f.span, "B026", ".h 只声明接口，函数体放在 .b");
        }
        let body = implementation
            .functions
            .iter()
            .find(|x| x.name == f.name)
            .ok_or_else(|| Diagnostic {
                source: interface.source.clone(),
                span: f.span,
                code: "B027",
                message: format!("接口 {} 缺少 .b 实现", f.name),
            })?;
        if !same_signature(f, body) {
            return implementation.source.fail(
                body.span,
                "B028",
                format!(
                    "{} 的函数效果、类型参数、访问权限、类型、参数名、执行目标或前缀域约束与 .h 不一致",
                    f.name
                ),
            );
        }
    }
    for (f, source, public) in interface
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Constant || f.external.is_some())
        .map(|f| (f, &interface.source, true))
        .chain(implementation.functions.iter().map(|f| {
            (
                f,
                &implementation.source,
                interface.functions.iter().any(|x| x.name == f.name),
            )
        }))
    {
        if f.body.is_none() && f.external.is_none() {
            return source.fail(f.span, "B029", ".b 中的函数必须有实现");
        }
        let key = format!("{name}::{}", f.name);
        if world.functions.contains_key(&key) {
            return source.fail(f.span, "B030", format!("值名称重复 {key}"));
        }
        world.functions.insert(
            key,
            Arc::new(Definition {
                function: f.clone(),
                source: source.clone(),
                module: name.clone(),
                imports: imports.clone(),
                public,
            }),
        );
    }
    visiting.remove(&name);
    loaded.insert(name.clone());
    Ok(name)
}
impl World {
    fn resolve(
        &self,
        name: &str,
        module: &str,
        imports: &[String],
        functions: bool,
    ) -> Option<String> {
        resolve_name(name, module, imports, |n| {
            if functions {
                self.functions.get(n).is_some_and(|d| {
                    d.module == module || (d.public && imports.contains(&d.module))
                })
            } else {
                self.records.contains_key(n)
            }
        })
    }
    fn ty(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
    ) -> Result<ir::Ty> {
        self.ty_inner(name, source, span, module, imports, &mut BTreeSet::new())
    }
    fn ty_inner(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<ir::Ty> {
        if name.name == "texture2d" {
            return source.fail(
                span,
                "B239",
                "texture2d 只能作为 read 资源参数，不能作为普通值存储或返回",
            );
        }
        if name.name == "void" {
            return source.fail(span, "B228", "void 只能表示函数无返回值，不能作为数据类型");
        }
        if name.tile {
            return source.fail(
                span,
                "B207",
                "tile<T> 只能作为 cooperative 函数的只读借用参数，不能存储或返回",
            );
        }
        if name.view {
            let mut element = name.clone();
            element.view = false;
            let element = self.ty_inner(&element, source, span, module, imports, stack)?;
            return Ok(ir::Ty {
                name: format!("view<{}>", element.name),
                arguments: Vec::new(),
                values: Vec::new(),
                size: 16,
                align: 8,
                components: vec![
                    (ir::Scalar::U32, 0),
                    (ir::Scalar::U32, 4),
                    (ir::Scalar::U32, 8),
                ],
                fields: Vec::new(),
                view: Some(Box::new(element)),
                array: None,
            });
        }
        if name.name == "array" {
            if name.arguments.len() != 2 {
                return source.fail(span, "B238", "array<T,N> 需要元素类型和固定长度");
            }
            let GenericArgument::Type(element) = &name.arguments[0] else {
                return source.fail(span, "B238", "array 首参数需要值类型");
            };
            if element.dataset || element.tile {
                return source.fail(span, "B238", "array 不能包含 dataset 或 tile 句柄");
            }
            let element = self.ty_inner(element, source, span, module, imports, stack)?;
            let length = name.arguments[1].shape().ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B238",
                message: "array 长度需要 u32 字面量或 const 名称".into(),
            })?;
            let count = self
                .shape(&Some(length), source, span, module, imports)?
                .unwrap();
            if count == 0 {
                return source.fail(span, "B238", "普通值 array 的固定长度必须为正");
            }
            let stride = ir::align_up(element.size, element.align);
            let size = stride.checked_mul(count).ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B238",
                message: "array 布局超过 u32 字节地址范围".into(),
            })?;
            return Ok(ir::Ty {
                name: format!("array<{},{}>", element.name, count),
                arguments: Vec::new(),
                values: vec![count],
                size,
                align: element.align,
                components: (0..count)
                    .flat_map(|i| {
                        element
                            .components
                            .iter()
                            .map(move |(s, o)| (*s, i * stride + o))
                    })
                    .collect(),
                fields: Vec::new(),
                view: None,
                array: Some(Box::new(element)),
            });
        }
        if let Some(ty) = ir::Ty::builtin(&name.name) {
            if !name.arguments.is_empty() {
                return source.fail(span, "B232", "内置数值类型不接受记录类型实参");
            }
            return Ok(ty);
        }
        let Some(key) = self.resolve(&name.name, module, imports, false) else {
            return source.fail(span, "B031", format!("未知或不明确的类型 {}", name.name));
        };
        let (record, s, m, i) = &self.records[&key];
        if record.types.len() + record.values.len() != name.arguments.len() {
            return source.fail(
                span,
                "B232",
                format!(
                    "类型 {key} 需要 {} 个类型及 const 实参，实际为 {}",
                    record.types.len() + record.values.len(),
                    name.arguments.len()
                ),
            );
        }
        let mut arguments = Vec::new();
        let mut replacements = BTreeMap::new();
        for (parameter, arg) in record.types.iter().zip(&name.arguments) {
            let GenericArgument::Type(arg) = arg else {
                return source.fail(span, "B232", "此记录位置需要普通值类型");
            };
            if arg.dataset || arg.view || arg.tile {
                return source.fail(span, "B232", "记录类型实参不能直接是资源包装");
            }
            let actual = self.ty_inner(arg, source, span, module, imports, stack)?;
            let members = self.family_members(&parameter.family, s, record.span, m, i, stack)?;
            if !members.iter().any(|(_, member)| member.name == actual.name) {
                return source.fail(
                    span,
                    "B232",
                    format!("类型 {} 不属于 {} 的允许集合", actual.name, parameter.name),
                );
            }
            replacements.insert(parameter.name.clone(), actual.syntax());
            arguments.push(actual);
        }
        let mut values = Vec::new();
        let mut value_map = BTreeMap::new();
        for (parameter, arg) in record
            .values
            .iter()
            .zip(&name.arguments[record.types.len()..])
        {
            let shape = arg.shape().ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B235",
                message: "记录 const 实参需要 u32 字面量或 const 名称".into(),
            })?;
            let value = self
                .shape(&Some(shape), source, span, module, imports)?
                .unwrap();
            if parameter
                .choices
                .as_ref()
                .is_some_and(|choices| !choices.contains(&value))
            {
                return source.fail(
                    span,
                    "B235",
                    format!(
                        "记录 const 实参 {value} 不属于 {} 的允许值域",
                        parameter.name
                    ),
                );
            }
            values.push(value);
            value_map.insert(parameter.name.clone(), value);
        }
        let key = if arguments.is_empty() && values.is_empty() {
            key
        } else {
            generics::key(
                &key,
                &arguments
                    .iter()
                    .map(|t| t.name.clone())
                    .chain(values.iter().map(u32::to_string))
                    .collect::<Vec<_>>(),
            )
        };
        if !stack.insert(key.clone()) {
            return source.fail(span, "B032", "结构体不能递归包含自身");
        }
        let mut ty = ir::Ty {
            name: key.clone(),
            arguments,
            values,
            size: 0,
            align: 4,
            components: Vec::new(),
            fields: Vec::new(),
            view: None,
            array: None,
        };
        let mut names = BTreeSet::new();
        for field in &record.fields {
            if field.ty.dataset {
                return s.fail(field.span, "B033", "数据集句柄不能嵌入设备结构体");
            }
            if !names.insert(field.name.clone()) {
                return s.fail(field.span, "B034", format!("重复字段 {}", field.name));
            }
            let mut field_type = field.ty.clone();
            generics::substitute_type(&mut field_type, &replacements, s, field.span)?;
            value_parameters::substitute_type(&mut field_type, &value_map);
            let f = self.ty_inner(&field_type, s, field.span, m, i, stack)?;
            let offset = (ty.size as u64).div_ceil(f.align as u64) * f.align as u64;
            let end = offset + f.size as u64;
            if end > u32::MAX as u64 {
                return s.fail(field.span, "B238", "记录布局超过 u32 字节地址范围");
            }
            let offset = offset as u32;
            ty.components
                .extend(f.components.iter().map(|(s, o)| (*s, o + offset)));
            ty.size = end as u32;
            ty.align = ty.align.max(f.align);
            ty.fields.push((field.name.clone(), offset, Box::new(f)));
        }
        if ty.components.is_empty() {
            return source.fail(span, "B035", "空结构体不能作为计算数据");
        }
        let size = (ty.size as u64).div_ceil(ty.align as u64) * ty.align as u64;
        ty.size = u32::try_from(size).map_err(|_| Diagnostic {
            source: source.clone(),
            span,
            code: "B238",
            message: "记录对齐后超过 u32 字节地址范围".into(),
        })?;
        if let Some(shape) = &record.stride {
            let stride = match shape {
                Shape::Constant(name) if value_map.contains_key(name) => value_map[name],
                _ => self.shape(&record.stride, s, record.span, m, i)?.unwrap(),
            };
            if stride < ty.size || stride % ty.align != 0 {
                return s.fail(
                    record.span,
                    "B241",
                    format!(
                        "记录 stride {stride} 必须容纳自然布局 {} 且是对齐 {} 的倍数",
                        ty.size, ty.align
                    ),
                );
            }
            ty.size = stride;
        }
        stack.remove(&key);
        Ok(ty)
    }
}

fn resolve_name(
    name: &str,
    module: &str,
    imports: &[String],
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    if name.contains("::") && exists(name) {
        return Some(name.into());
    }
    let local = format!("{module}::{name}");
    if exists(&local) {
        return Some(local);
    }
    let candidates: Vec<_> = imports
        .iter()
        .map(|m| format!("{m}::{name}"))
        .filter(|n| exists(n))
        .collect();
    if candidates.len() == 1 {
        Some(candidates[0].clone())
    } else {
        None
    }
}
