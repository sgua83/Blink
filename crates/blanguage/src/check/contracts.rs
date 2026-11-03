//! 函数约束只精化原参数 Proof；调用点证明真实前提，不生成运行期检查。
use super::*;

pub(super) fn root_lengths(
    env: &BTreeMap<String, Root>,
    definition: &Definition,
) -> Result<Vec<(usize, usize)>> {
    let mut relations = BTreeSet::new();
    for contract in &definition.function.contracts {
        let ContractPredicate::SameLength { left, right } = &contract.predicate else { continue; };
        if contract.kind != ContractKind::Requires {
            return definition.source.fail(contract.span, "B225", "same_length声明输入dataset的requires前提");
        }
        let (Some(Root::Data(a, _)), Some(Root::Data(b, _))) = (env.get(left), env.get(right)) else {
            return definition.source.fail(contract.span, "B225", "same_length需要两个dataset形参");
        };
        relations.insert(((*a).min(*b), (*a).max(*b)));
    }
    Ok(relations.into_iter().collect())
}
struct Length {
    root: String,
    proof: bounds::Proof,
}
fn length(
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
    name: &str,
    span: Span,
) -> Result<Length> {
    let proof = match env.get(name) {
        Some(Binding::ApplicationData(data)) => bounds::Proof::capture(&data.length),
        Some(Binding::DataReference(data)) => bounds::Proof::capture(&data.length),
        Some(Binding::Data(data)) => data.length.clone(),
        _ => return definition.source.fail(span, "B225", "same_length需要两个dataset形参"),
    };
    Ok(Length { root: name.into(), proof })
}
fn assume_length(name: &str, binding: &mut Binding, selected: &Length, other: &Length) {
    if name != selected.root { return; }
    match binding {
        Binding::ApplicationData(data) => data.length.proof.assume_same_length(&other.proof),
        Binding::DataReference(data) => data.data.length.proof.assume_same_length(&other.proof),
        Binding::Data(data) => data.length.assume_same_length(&other.proof),
        _ => {}
    }
}
struct Selection {
    root: String,
    components: Vec<usize>,
    value: Value,
    element_paths: Vec<Vec<usize>>,
    empty: bool,
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
    Value(Selection),
}
impl Resource {
    fn domain(&self) -> IndexDomain {
        match self {
            Self::Application { handle, .. } => IndexDomain::ApplicationData(*handle),
            Self::Array(count) => IndexDomain::Fixed(*count),
            Self::Data { binding, .. } => IndexDomain::Data(*binding),
            Self::Tile { .. } => IndexDomain::Tile,
            Self::Value(value) if value.value.ty.array.is_some() => IndexDomain::Fixed(value.value.ty.values[0]),
            Self::Value(value) if value.value.ty.dataset.is_some() =>
                IndexDomain::ApplicationData(value.value.identity()),
            Self::Value(value) => IndexDomain::View(value.value.view_identity()),
        }
    }
    fn minimum(&self) -> u32 {
        match self {
            Self::Application { minimum, .. } => *minimum,
            Self::Array(count) => *count,
            Self::Data { minimum, .. } | Self::Tile { minimum, .. } => *minimum,
            Self::Value(value) if value.value.ty.array.is_some() => value.value.ty.values[0],
            Self::Value(value) if value.value.ty.dataset.is_some() => value.value.ty.values
                .first().copied().unwrap_or_else(||bounds::Proof::capture(&value.value).unsigned_minimum(1)),
            Self::Value(value) => Builder::view_length(&value.value).minimum(),
        }
    }
    fn empty(&self)->bool {
        match self {Self::Value(value)=>value.empty,_=>false}
    }
    fn assume_nonempty(&self, name: &str, binding: &mut Binding) {
        match (self, binding) {
            (Self::Application { root, .. }, Binding::ApplicationData(data)) if root == name => {
                data.length.proof.mark_nonzero(0);
            }
            (Self::Application { root, .. }, Binding::DataReference(data)) if root == name => {
                data.data.length.proof.mark_nonzero(0);
            }
            (Self::Data { root, .. }, Binding::Data(view)) if root == name => {
                view.length.mark_nonzero(0);
            }
            (Self::Tile { root, .. }, Binding::Tile(view)) if root == name => {
                view.length.mark_nonzero(0);
            }
            (Self::Value(selection), binding) if selection.root == name => {
                selection.assume_nonempty(binding);
            }
            _ => {}
        }
    }
}
enum Predicate {
    SameLength(Length, Length),
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
    definition: &Definition, env: &BTreeMap<String, Binding>, path: &str, span: Span,
) -> Result<Selection> {
    if path.contains("[*]"){return select_elements(definition,env,path,span);}
    let root = path.split('.').next().unwrap();
    let value = match env.get(root) {
        Some(Binding::Value(value)) => value,
        Some(Binding::OwnedValue(value))
            if definition.function.params.iter().any(|parameter|
                parameter.name == root && parameter.access == Access::Value) => &value.value,
        _ => return definition.source.fail(span, "B225", "值约束需要按值参数或其字段；不能读取原子位置或数据集元素"),
    };
    select_value(value, path, &definition.source, span)
}
fn select_value(value: &Value, path: &str, source: &Source, span: Span) -> Result<Selection> {
    let mut fields = path.split('.');
    let root = fields.next().unwrap();
    let mut value = value.clone();
    let mut components: Vec<_> = (0..value.regs.len()).collect();
    for field in fields {
        let (ty, indices) = member_type(&value.ty, field, source, span)
            .map_err(|error| Diagnostic {
                source: source.clone(),
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
        root: root.into(),components,value,element_paths:Vec::new(),empty:false,
    })
}

fn selected_data<'a>(binding:&'a Binding,paths:&[Vec<usize>])->Option<&'a application::Data> {
    let(first,rest)=paths.split_first()?;
    let mut data=match binding {
        Binding::ApplicationData(data) if first.is_empty()=>data,
        Binding::DataReference(data) if first.is_empty()=>&data.data,
        Binding::OwnedValue(value)=>view::owned_data_at(value,first)?,
        _=>return None,
    };
    for path in rest {data=application::model_data(&data.elements,path)?;}
    Some(data)
}
fn selected_data_mut<'a>(binding:&'a mut Binding,paths:&[Vec<usize>])->Option<&'a mut application::Data> {
    let(first,rest)=paths.split_first()?;
    let mut data=match binding {
        Binding::ApplicationData(data) if first.is_empty()=>data,
        Binding::DataReference(data) if first.is_empty()=>&mut data.data,
        Binding::OwnedValue(value)=>view::owned_data_at_mut(value,first)?,
        _=>return None,
    };
    for path in rest {data=application::model_data_mut(&mut data.elements,path)?;}
    Some(data)
}
impl Selection {

    fn assume_empty_scope(&self,binding:&mut Binding) {
        fn empty(data:&mut application::Data)->bounds::Proof {
            let mut zero=data.length.clone();zero.constant=Some(vec![0]);
            let proof=bounds::Proof::capture(&zero);
            data.length.proof.assume_same_length(&proof);proof
        }
        fn owned(value:&mut owned_values::OwnedValue,path:&[usize]) {
            if let Some(data)=view::owned_data_at_mut(value,path) {
                let proof=empty(data);
                let mut ty=&value.value.ty;let mut offset=0;
                for field in path {let(_,at,next)=&ty.fields[*field];offset+=*at;ty=next;}
                offset+=ty.components[1].1;
                let component=value.value.ty.components.iter().position(|(_,at)|*at==offset).unwrap();
                let mut length=value.value.projected_proof(&[component]);length.assume_same_length(&proof);
                value.value.proof.assign(&[component],value.value.regs.len(),length);
            }
        }
        let Some((last,parents))=self.element_paths.split_last()else{return;};
        if parents.is_empty() {
            if let Binding::OwnedValue(value)=binding {owned(value,last);}
            else if let Some(data)=selected_data_mut(binding,&self.element_paths){empty(data);}
        }else if let Some(data)=selected_data_mut(binding,parents)
            && let returns::Returned::OwnedValue(value)=data.elements.as_mut() {owned(value,last);}
    }
    fn assume_owned_nonempty(&self,value:&mut owned_values::OwnedValue,component:usize) {
        value.value.proof.mark_nonzero(component);
        if self.value.ty.dataset.is_some() {
            let identity=self.value.identity();
            value.resources_mut(&mut |data| {
                if data.handle.identity()==identity {data.length.proof.mark_nonzero(0);}
            });
        }
    }
    fn assume_nonempty(&self,binding:&mut Binding) {
        if self.value.ty.array.is_some() {
            if self.value.ty.values[0]==0 {self.assume_empty_scope(binding);}
            return;
        }
        let component=self.components[if self.value.ty.dataset.is_some(){1}else{2}];
        if !self.element_paths.is_empty() {
            if let Some(data)=selected_data_mut(binding,&self.element_paths) {
                match data.elements.as_mut() {
                    returns::Returned::OwnedValue(value)=>self.assume_owned_nonempty(value,component),
                    model=>{if let Some(value)=model.view_value_mut(){value.proof.mark_nonzero(component);}},
                }
            }
            return;
        }
        match binding {
            Binding::Value(value)=>value.proof.mark_nonzero(component),
            Binding::OwnedValue(value)=>{
                value.value.proof=bounds::Proof::capture(&value.value);
                self.assume_owned_nonempty(value,component);
            },
            _=>{},
        }
    }
    fn assumed_value<'a>(&self,binding:&'a mut Binding)->Option<&'a mut Value> {
        if !self.element_paths.is_empty(){return selected_data_mut(binding,&self.element_paths)?.elements.view_value_mut();}
        match binding {
            Binding::Value(value)=>Some(value),
            Binding::OwnedValue(value)=>{value.value.proof=bounds::Proof::capture(&value.value);Some(&mut value.value)},
            _=>None,
        }
    }
}
fn field_path(mut ty:&Ty,path:&str,source:&Source,span:Span)->Result<Vec<usize>> {
    let mut result=Vec::new();
    for field in path.trim_start_matches('.').split('.').filter(|field|!field.is_empty()) {
        let Some(index)=ty.fields.iter().position(|(name,..)|name==field)else{return source.fail(span,"B225",format!("共同元素资源路径需要实际dataset字段：{path}"));};
        result.push(index);ty=&ty.fields[index].2;
    }
    Ok(result)
}
fn select_elements(definition:&Definition,env:&BTreeMap<String,Binding>,path:&str,span:Span)->Result<Selection> {
    let parts=path.split("[*]").collect::<Vec<_>>();let first=parts[0];
    let(root,fields)=first.split_once('.').unwrap_or((first,""));
    let source=&definition.source;
    if !definition.function.params.iter().any(|p|p.name==root&&matches!(p.access,Access::Value|Access::Read|Access::InOut)) {
        return source.fail(span,"B225","共同元素约束需要读取实际普通CPU dataset输入");
    }
    let Some(binding)=env.get(root)else{return source.fail(span,"B225","共同元素约束需要实际dataset参数");};
    let first_path=match binding {
        Binding::OwnedValue(value)=>field_path(&value.value.ty,fields,source,span)?,
        Binding::ApplicationData(_)|Binding::DataReference(_) if fields.is_empty()=>Vec::new(),
        _=>return source.fail(span,"B225","[*]需要实际CPU dataset存储及其共同元素模型"),
    };
    let mut paths=vec![first_path];
    let Some(mut data)=selected_data(binding,&paths)else{return source.fail(span,"B225","[*]需要实际dataset字段");};
    let mut projection=String::new();
    if let Binding::DataReference(reference)=binding {
        let mut ty=&reference.data.ty;
        for field in &reference.projection {let(name,_,child)=&ty.fields[*field];projection.push('.');projection.push_str(name);ty=child;}
    }
    let mut empty=data.length.maximum()==0;
    for part in &parts[1..parts.len()-1] {
        let selected=field_path(&data.ty,&(projection.clone()+part),source,span)?;
        let Some(next)=application::model_data(&data.elements,&selected)else{return source.fail(span,"B225","[*]需要所选共同元素中的实际dataset字段");};
        paths.push(selected);data=next;empty|=data.length.maximum()==0;projection.clear();
    }
    let Some(value)=data.elements.view_value()else{return source.fail(span,"B225","约束需要选择共同元素的实际值／字段");};
    let mut selection=select_value(value,&(root.to_owned()+&projection+parts.last().unwrap()),source,span)?;
    selection.element_paths=paths;selection.empty=empty;Ok(selection)
}

fn validate_nonzero_type(ty: &Ty, source: &Source, span: Span) -> Result<()> {
    if !(ty.is_numeric() || ty.is_c_function())
        || !ty.components.first().is_some_and(|(scalar, _)| scalar.is_integer()) {
        return source.fail(span, "B225", "nonzero 需要整数标量／向量或cfn函数地址的按值参数／字段");
    }
    Ok(())
}
pub(super) fn validate_nonzero_input(path: &str,
    input: &impl Fn(&str) -> Option<(Access, Ty)>, source: &Source, span: Span,
) -> Result<()> {
    let mut fields = path.split('.');
    let root = fields.next().unwrap();
    let Some((Access::Value, mut ty)) = input(root) else {
        return source.fail(span, "B225", "nonzero需要实际按值输入或其字段");
    };
    for field in fields { ty = member_type(&ty, field, source, span)?.0; }
    validate_nonzero_type(&ty, source, span)
}
pub(super) fn prove_c_inputs(builder: &Builder<'_>, contract: outputs::Contract<'_>,
    inputs: &BTreeMap<String, Value>, span: Span,
) -> Result<()> {
    let requirements = contract.nonzero_inputs().map(|(path, at)| {
        let root = path.split('.').next().unwrap();
        let value = inputs.get(root).ok_or_else(|| Diagnostic {
            source: contract.source.clone(), span: at, code: "B225",
            message: "nonzero需要实际按值输入或其字段".into(),
        })?;
        let selection = select_value(value, path, contract.source, at)?;
        validate_nonzero_type(&selection.value.ty, contract.source, at)?;
        Ok(Requirement { predicate: Predicate::Nonzero(selection),
            description: format!("requires nonzero({path})") })
    }).collect::<Result<Vec<_>>>()?;
    prove(builder, &requirements, span)
}

fn resource(
    builder: &Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
    path: &str,
    span: Span,
) -> Result<Resource> {
    match env.get(path) {
        Some(Binding::DataReference(data))=>{
            return Ok(Resource::Application{root:path.into(),handle:data.handle.identity(),minimum:data.length.minimum()});
        }
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
    let selection = select(definition, env, path, span)?;
    if selection.value.ty.array.is_some() && selection.element_paths.is_empty() {
        return Ok(Resource::Array(selection.value.ty.values[0]));
    }
    if selection.value.ty.array.is_none() && selection.value.ty.dataset.is_none() && selection.value.ty.view.is_none() {
        return definition.source.fail(
            span,
            "B225",
            "资源约束需要 dataset、tile 或不可变 view 参数／字段",
        );
    }
    Ok(Resource::Value(selection))
}

pub(super) fn resolve(
    builder: &Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
) -> Result<Vec<Requirement>> {
    if !definition.function.c_abi { outputs::validate(builder.world, definition)?; }
    let mut result = Vec::new();
    for contract in &definition.function.contracts {
        if matches!(contract.predicate, ContractPredicate::Initialized { .. }) {
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
            ContractPredicate::SameLength { left, right } => (
                Predicate::SameLength(length(definition, env, left, contract.span)?, length(definition, env, right, contract.span)?),
                format!("same_length({left},{right})"),
            ),
            ContractPredicate::Contiguous(path) => (
                Predicate::Contiguous(reference(builder, definition, env, path, contract.span)?),
                format!("contiguous({path})"),
            ),
            ContractPredicate::Nonzero(path) => {
                let selection = select(definition, env, path, contract.span)?;
                validate_nonzero_type(&selection.value.ty, &definition.source, contract.span)?;
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
                let selection = select(definition, env, value, contract.span)?;
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
            Predicate::SameLength(left, right) => builder.length_proofs_equal(&left.proof, &right.proof),
            Predicate::Contiguous(place) => builder.reference_is_contiguous(place, span)?,
            Predicate::Nonzero(selection) => selection.empty || selection.value.is_nonzero(),
            Predicate::Nonempty(resource) => {
                resource.empty() || builder.resource_minimum(resource.domain(), resource.minimum()) != 0
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
                Predicate::SameLength(left, right) => {
                    assume_length(name, binding, left, right);
                    assume_length(name, binding, right, left);
                }
                Predicate::Contiguous(_) => {}
                Predicate::Nonzero(selection) => {
                    if selection.root == *name && let Some(value)=selection.assumed_value(binding) {
                        for component in &selection.components {value.proof.mark_nonzero(*component);}
                    }
                }
                Predicate::Nonempty(resource) => resource.assume_nonempty(name, binding),
                Predicate::Index(selection, resource) => {
                    if selection.root == *name {
                        let value = match binding {
                            Binding::Value(value) => value,
                            Binding::OwnedValue(value) => {
                                value.value.proof = bounds::Proof::capture(&value.value);
                                &mut value.value
                            },
                            _ => continue,
                        };
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
        let (ty, components) = member_type(place.ty(), field, &builder.definition.source, span)?;
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
            .map(|(_, offset)| *offset)
            .collect::<Vec<_>>();
        for field in fields {
            let (next, components) = member_type(&selected, field, &builder.definition.source, span)?;
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
