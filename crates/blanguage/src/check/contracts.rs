//! 函数约束只精化原参数 Proof；调用点证明真实前提，不生成运行期检查。
use super::*;

struct Selection {
    root: String,
    components: Vec<usize>,
    value: Value,
}
enum Resource {
    Application {
        root: String,
        handle: ValueIdentity,
        minimum: u32,
    },
    Array(u32),
    Data {
        root: String,
        binding: usize,
        minimum: u32,
    },
    Tile {
        root: String,
        minimum: u32,
    },
    View(Selection),
}
impl Resource {
    fn domain(&self) -> IndexDomain {
        match self {
            Self::Application { handle, .. } => IndexDomain::ApplicationData(*handle),
            Self::Array(count) => IndexDomain::Fixed(*count),
            Self::Data { binding, .. } => IndexDomain::Data(*binding),
            Self::Tile { .. } => IndexDomain::Tile,
            Self::View(value) => IndexDomain::View(value.value.view_identity()),
        }
    }
    fn minimum(&self) -> u32 {
        match self {
            Self::Application { minimum, .. } => *minimum,
            Self::Array(count) => *count,
            Self::Data { minimum, .. } | Self::Tile { minimum, .. } => *minimum,
            Self::View(value) => Builder::view_length(&value.value).minimum(),
        }
    }
    fn assume_nonempty(&self, name: &str, binding: &mut Binding) {
        match (self, binding) {
            (Self::Application { root, .. }, Binding::ApplicationData(data)) if root == name => {
                data.length.proof.mark_nonzero(0);
            }
            (Self::Data { root, .. }, Binding::Data(view)) if root == name => {
                view.length.mark_nonzero(0);
            }
            (Self::Tile { root, .. }, Binding::Tile(view)) if root == name => {
                view.length.mark_nonzero(0);
            }
            (Self::View(selection), Binding::Value(value)) if selection.root == name => {
                value.proof.mark_nonzero(selection.components[2]);
            }
            _ => {}
        }
    }
}
enum Predicate {
    Contiguous(Place),
    Nonzero(Selection),
    Nonempty(Resource),
    Index(Selection, Resource),
}
pub(super) struct Requirement {
    predicate: Predicate,
    description: String,
}

fn select(
    builder: &Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
    path: &str,
    span: Span,
) -> Result<Selection> {
    let mut fields = path.split('.');
    let root = fields.next().unwrap();
    let Some(Binding::Value(value)) = env.get(root) else {
        return definition.source.fail(
            span,
            "B225",
            "值约束需要按值参数或其字段；不能读取原子位置或数据集元素",
        );
    };
    let mut value = value.clone();
    let mut components: Vec<_> = (0..value.regs.len()).collect();
    for field in fields {
        let (ty, indices) = builder
            .member(&value.ty, field, span)
            .map_err(|error| Diagnostic {
                source: definition.source.clone(),
                span,
                code: "B225",
                message: format!("约束字段 {path}：{}", error.message),
            })?;
        components = indices.iter().map(|i| components[*i]).collect();
        value = Value {
            proof: value.projected_proof(&indices),
            literal: false,
            ty,
            regs: indices.iter().map(|i| value.regs[*i]).collect(),
            uniform: value.uniform,
            constant: value
                .constant
                .as_ref()
                .map(|v| indices.iter().map(|i| v[*i]).collect()),
        };
    }
    Ok(Selection {
        root: root.into(),
        components,
        value,
    })
}
fn resource(
    builder: &Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
    path: &str,
    span: Span,
) -> Result<Resource> {
    match env.get(path) {
        Some(Binding::ApplicationData(data)) => {
            return Ok(Resource::Application {
                root: path.into(),
                handle: data.handle.identity(),
                minimum: data.length.minimum(),
            });
        }
        Some(Binding::Texture(reference)) => {
            if let Some(count) = reference.shape(builder.program) {
                return Ok(Resource::Array(count));
            }
        }
        Some(Binding::Data(view)) => {
            return Ok(Resource::Data {
                root: path.into(),
                binding: view.binding,
                minimum: view.length.unsigned_minimum(0),
            });
        }
        Some(Binding::Tile(view)) => {
            return Ok(Resource::Tile {
                root: path.into(),
                minimum: view.length.unsigned_minimum(0),
            });
        }
        _ => {}
    }
    // 固定长度只来自类型；原子／独占借用的数组约束不读取其内容。
    let (root, field) = path
        .split_once('.')
        .map_or((path, None), |(r, f)| (r, Some(f)));
    let ty = match env.get(root) {
        Some(Binding::AtomicBorrow(place)) => Some(place.ty()),
        Some(Binding::Owned(owner)) => Some(owner.place.ty()),
        _ => None,
    };
    if let Some(ty) = ty {
        let selected = if let Some(field) = field {
            ty.field(field).map(|(_, ty)| ty)
        } else {
            Some(ty)
        };
        if let Some(ty) = selected
            && ty.array.is_some()
        {
            return Ok(Resource::Array(ty.values[0]));
        }
    }
    let selection = select(builder, definition, env, path, span)?;
    if selection.value.ty.array.is_some() {
        return Ok(Resource::Array(selection.value.ty.values[0]));
    }
    if selection.value.ty.view.is_none() {
        return definition.source.fail(
            span,
            "B225",
            "资源约束需要 dataset、tile 或不可变 view 参数／字段",
        );
    }
    Ok(Resource::View(selection))
}

pub(super) fn resolve(
    builder: &Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
) -> Result<Vec<Requirement>> {
    let mut result = Vec::new();
    for contract in &definition.function.contracts {
        if matches!(contract.predicate, ContractPredicate::Initialized { .. }) {
            outputs::validate(builder, definition, contract)?;
            continue;
        }
        if contract.kind != ContractKind::Requires
            && !matches!(contract.predicate, ContractPredicate::Prefix { .. })
        {
            return definition.source.fail(
                contract.span,
                "B225",
                "普通／cooperative 函数当前只接受 requires 值约束，不接受 ensures",
            );
        }
        let (predicate, description) = match &contract.predicate {
            ContractPredicate::Contiguous(path) => (
                Predicate::Contiguous(reference(builder, definition, env, path, contract.span)?),
                format!("contiguous({path})"),
            ),
            ContractPredicate::Nonzero(path) => {
                let selection = select(builder, definition, env, path, contract.span)?;
                if !selection.value.ty.is_numeric()
                    || !matches!(
                        selection.value.ty.components.first(),
                        Some((Scalar::U32 | Scalar::I32, _))
                    )
                {
                    return definition.source.fail(
                        contract.span,
                        "B225",
                        "nonzero 需要 u32/i32 标量或向量的按值参数／字段",
                    );
                }
                (Predicate::Nonzero(selection), format!("nonzero({path})"))
            }
            ContractPredicate::Nonempty(path) => (
                Predicate::Nonempty(resource(builder, definition, env, path, contract.span)?),
                format!("nonempty({path})"),
            ),
            ContractPredicate::Index {
                value,
                resource: path,
            } => {
                let selection = select(builder, definition, env, value, contract.span)?;
                if !selection.value.ty.is_numeric()
                    || !matches!(
                        selection.value.ty.components.first(),
                        Some((Scalar::U32, _))
                    )
                {
                    return definition.source.fail(
                        contract.span,
                        "B225",
                        "index 需要 u32 标量或向量的按值参数／字段",
                    );
                }
                let resource = resource(builder, definition, env, path, contract.span)?;
                (
                    Predicate::Index(selection, resource),
                    format!("index({value},{path})"),
                )
            }
            ContractPredicate::Prefix { .. } => {
                return definition.source.fail(
                    contract.span,
                    "B192",
                    "前缀域约束只用于 program 接口",
                );
            }
            ContractPredicate::Initialized { .. } => unreachable!(),
        };
        result.push(Requirement {
            predicate,
            description: format!("{} 的 requires {description}", definition.function.name),
        });
    }
    Ok(result)
}

pub(super) fn prove(builder: &Builder<'_>, requirements: &[Requirement], span: Span) -> Result<()> {
    for requirement in requirements {
        let holds = match &requirement.predicate {
            Predicate::Contiguous(place) => builder.reference_is_contiguous(place, span)?,
            Predicate::Nonzero(selection) => selection.value.is_nonzero(),
            Predicate::Nonempty(resource) => {
                builder.resource_minimum(resource.domain(), resource.minimum()) != 0
            }
            Predicate::Index(selection, resource) => {
                let value = &selection.value;
                (0..value.regs.len()).all(|i| {
                    builder.index_in(
                        &Value {
                            proof: value.projected_proof(&[i]),
                            literal: false,
                            ty: Ty::scalar(Scalar::U32),
                            regs: vec![value.regs[i]],
                            uniform: value.uniform,
                            constant: value.constant.as_ref().map(|v| vec![v[i]]),
                        },
                        resource.domain(),
                        resource.minimum(),
                    )
                })
            }
        };
        if !holds {
            return builder.fail(
                span,
                "B226",
                format!("调用前尚未证明 {}", requirement.description),
            );
        }
    }
    Ok(())
}
pub(super) fn assume(env: &mut BTreeMap<String, Binding>, requirements: &[Requirement]) {
    for requirement in requirements {
        for (name, binding) in env.iter_mut() {
            match &requirement.predicate {
                Predicate::Contiguous(_) => {}
                Predicate::Nonzero(selection) => {
                    if let Binding::Value(value) = binding
                        && selection.root == *name
                    {
                        for component in &selection.components {
                            value.proof.mark_nonzero(*component);
                        }
                    }
                }
                Predicate::Nonempty(resource) => resource.assume_nonempty(name, binding),
                Predicate::Index(selection, resource) => {
                    if let Binding::Value(value) = binding
                        && selection.root == *name
                    {
                        for component in &selection.components {
                            value.proof.mark_domain(*component, resource.domain());
                        }
                    }
                    resource.assume_nonempty(name, binding);
                }
            }
        }
    }
}

fn reference(
    builder: &Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
    path: &str,
    span: Span,
) -> Result<Place> {
    let mut fields = path.split('.');
    let root = fields.next().unwrap();
    if !definition.function.params.iter().any(|param| {
        param.name == root
            && matches!(param.access, Access::Write | Access::InOut)
            && !param.ty.dataset
    }) {
        return definition.source.fail(
            span,
            "B225",
            "contiguous需要write／inout普通位置参数或其字段",
        );
    }
    let Some(Binding::Owned(owner)) = env.get(root) else {
        return definition
            .source
            .fail(span, "B225", "contiguous需要实际的位置借用");
    };
    let mut place = owner.place.clone();
    for field in fields {
        let (ty, components) = builder.member(place.ty(), field, span)?;
        place = place.project(ty, &components);
    }
    Ok(place)
}

// Layout is stable under payload writes. Its sole assumption source is the current
// function's declared requires, so recursion needs no new inference registry or pass.
pub(super) fn contiguous_assumed(
    builder: &Builder<'_>,
    parameter: usize,
    words: &[u32],
    ty: &Ty,
    span: Span,
) -> Result<bool> {
    let definition = &builder.definition;
    let param = &definition.function.params[parameter];
    for contract in &definition.function.contracts {
        let ContractPredicate::Contiguous(path) = &contract.predicate else {
            continue;
        };
        if contract.kind != ContractKind::Requires {
            continue;
        }
        let mut fields = path.split('.');
        if fields.next().unwrap() != param.name {
            continue;
        }
        let mut selected = cooperative::parameter_type(builder.world, param, definition)?;
        let mut guaranteed = selected
            .components
            .iter()
            .map(|(_, offset)| offset / 4)
            .collect::<Vec<_>>();
        for field in fields {
            let (next, components) = builder.member(&selected, field, span)?;
            guaranteed = components.iter().map(|i| guaranteed[*i]).collect();
            selected = next;
        }
        let Some(positions) = words
            .iter()
            .map(|word| guaranteed.iter().position(|candidate| candidate == word))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let first = selected.components[positions[0]].1;
        if selected.align % ty.align == 0
            && first % ty.align == 0
            && positions
                .iter()
                .zip(&ty.components)
                .all(|(position, (_, offset))| {
                    selected.components[*position].1.checked_sub(first) == Some(*offset)
                })
        {
            return Ok(true);
        }
    }
    Ok(false)
}
