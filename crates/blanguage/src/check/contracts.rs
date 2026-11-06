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
    scope: Option<bounds::Condition>,
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
        match self {Self::Value(value)=>value.empty(),_=>false}
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
    Index(Selection, Resource, Option<(bounds::Condition, bounds::Condition)>),
    Cleanup { when: bounds::Condition, required: bounds::Condition },
}
pub(super) struct Requirement {
    predicate: Predicate,
    description: String,
}

fn select(
    builder: &Builder<'_>, definition: &Definition, env: &BTreeMap<String, Binding>, path: &str, span: Span,
) -> Result<Selection> {
    if path.contains("[*]"){return select_elements(builder,definition,env,path,span);}
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
        root: root.into(),components,value,element_paths:Vec::new(),scope:None,
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
    fn empty(&self) -> bool {
        self.scope.as_ref().is_some_and(|scope| scope.boolean_constant() == Some(false))
    }

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
                if data.root.identity()==identity {data.length.proof.mark_nonzero(0);}
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
fn select_elements(builder:&Builder<'_>,definition:&Definition,env:&BTreeMap<String,Binding>,path:&str,span:Span)->Result<Selection> {
    use std::borrow::Cow;
    let parts=path.split("[*]").collect::<Vec<_>>();let first=parts[0];
    let(root,fields)=first.split_once('.').unwrap_or((first,""));
    let source=&definition.source;
    if !definition.function.params.iter().any(|p|p.name==root&&matches!(p.access,Access::Value|Access::Read|Access::InOut)) {
        return source.fail(span,"B225","共同元素约束需要读取实际普通CPU dataset输入");
    }
    let Some(binding)=env.get(root)else{return source.fail(span,"B225","共同元素约束需要实际dataset参数");};
    let(elements,mut ty,mut paths,mut scope,projection)=match binding {
        Binding::Data(view) if fields.is_empty()=>{
            let ty=&builder.program.buffers[view.binding].ty;
            let mut model=builder.unknown_element_model(ty);
            let words=ty.components.iter().map(|(_,at)|*at).collect::<Vec<_>>();
            if let Some(common)=builder.common_element_proof(view.binding,&words)
                && let Some(value)=model.view_value_mut() {
                value.proof.constrain_common_ranges(&common,ty.components.len());
            }
            let scope=if view.length.unsigned_maximum(0)==0 {bounds::Condition::constant(false)}
                else if view.length.unsigned_minimum(0)!=0 {bounds::Condition::constant(true)}
                else {bounds::Condition::nonzero_proof(&view.length,0)};
            (Cow::Owned(*model),ty,vec![Vec::new()],scope,view.projection.as_slice())
        },
        _=>{
            let first_path=match binding {
                Binding::OwnedValue(value)=>field_path(&value.value.ty,fields,source,span)?,
                Binding::ApplicationData(_)|Binding::DataReference(_) if fields.is_empty()=>Vec::new(),
                _=>return source.fail(span,"B225","[*]需要实际CPU dataset存储及其共同元素模型"),
            };
            let paths=vec![first_path];
            let Some(data)=selected_data(binding,&paths)else{return source.fail(span,"B225","[*]需要实际dataset字段");};
            let projection=match binding {Binding::DataReference(reference)=>reference.projection.as_slice(),_=>&[]};
            (Cow::Borrowed(data.elements.as_ref()),&data.ty,paths,bounds::Condition::nonzero_value(&data.length),projection)
        },
    };
    let mut model=elements.as_ref();let mut projection_name=String::new();let mut field_ty=ty;
    for field in projection {let(name,_,child)=&field_ty.fields[*field];projection_name.push('.');projection_name.push_str(name);field_ty=child;}
    for part in &parts[1..parts.len()-1] {
        let selected=field_path(ty,&(projection_name.clone()+part),source,span)?;
        let Some(next)=application::model_data(model,&selected)else{return source.fail(span,"B225","[*]需要所选共同元素中的实际dataset字段");};
        paths.push(selected);scope=scope.logical(bounds::Condition::nonzero_value(&next.length),false);
        model=&next.elements;ty=&next.ty;projection_name.clear();
    }
    let Some(value)=model.view_value()else{return source.fail(span,"B225","约束需要选择共同元素的实际值／字段");};
    let mut selection=select_value(value,&(root.to_owned()+&projection_name+parts.last().unwrap()),source,span)?;
    selection.element_paths=paths;selection.scope=Some(scope);Ok(selection)
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
            return Ok(Resource::Application{root:path.into(),handle:data.root.identity(),minimum:data.length.minimum()});
        }
        Some(Binding::ApplicationData(data)) => {
            return Ok(Resource::Application {
                root: path.into(),
                handle: data.root.identity(),
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
    builder: &mut Builder<'_>,
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
            ContractPredicate::Memory(_)=>return definition.source.fail(contract.span,"B225","memory只用于外部C声明或cfn，普通B正文沿实际访问推导"),
            ContractPredicate::Contiguous(path) => (
                Predicate::Contiguous(reference(builder, definition, env, path, contract.span)?),
                format!("contiguous({path})"),
            ),
            ContractPredicate::Cleanup { value, condition } => {
                let inputs=outputs::inputs(builder,definition,env)?;
                let when=outputs::expression(builder,definition.into(),condition,&inputs,None,contract.span)?;
                let(model,active)=cleanup_selection(builder,definition,env,value,contract.span)?;
                let required=cleanup_condition(builder,model,active,contract.span)?;
                (Predicate::Cleanup{when:bounds::Condition::of(&when),required},format!("cleanup({value})"))
            },
            ContractPredicate::Nonzero(path) => {
                let selection = select(builder, definition, env, path, contract.span)?;
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
                let target = match &resource { Resource::Value(value) => Some(value), _ => None };
                if target.is_some_and(|value| !value.element_paths.is_empty())
                    && !target.is_some_and(|value| selection.root == value.root && selection.element_paths == value.element_paths)
                {
                    return definition.source.fail(contract.span, "B225", "共同元素index的值与资源需要同一dataset元素作用域");
                }
                let scope = if !selection.element_paths.is_empty()
                    && target.is_none_or(|value| value.element_paths.is_empty()) {
                    Some((selection.scope.as_ref().unwrap().clone(), nonempty_condition(builder, definition, env, &resource, contract.span)?))
                } else { None };
                (
                    Predicate::Index(selection, resource, scope),
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

fn holds(builder:&Builder<'_>,requirement:&Requirement,span:Span)->Result<bool>{
    Ok(match &requirement.predicate {
            Predicate::Cleanup{when,required}=>builder.condition_truth(&when.clone().negate().logical(required.clone(),true),None)==Some(true),
            Predicate::SameLength(left, right) => builder.length_proofs_equal(&left.proof, &right.proof),
            Predicate::Contiguous(place) => builder.reference_is_contiguous(place, span)?,
            Predicate::Nonzero(selection) => selection.empty() || selection.value.is_nonzero(),
            Predicate::Nonempty(resource) => {
                resource.empty() || builder.resource_minimum(resource.domain(), resource.minimum()) != 0
            }
            Predicate::Index(selection, resource, _) => {
                let value = &selection.value;
                let length = match resource {
                    Resource::Value(target) if target.value.ty.dataset.is_some() => Some(target.value.projected_proof(&[1])),
                    Resource::Value(target) if target.value.ty.view.is_some() => Some(target.value.projected_proof(&[2])),
                    _ => None,
                };
                selection.empty() || (0..value.regs.len()).all(|i| {
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
                        length.as_ref(),
                    )
                })
            }
        })
}

pub(super) fn prove(builder: &Builder<'_>, requirements: &[Requirement], span: Span) -> Result<()> {
    for requirement in requirements {

        if !holds(builder,requirement,span)? {
            return builder.fail(
                span,
                "B226",
                format!("调用前尚未证明 {}", requirement.description),
            );
        }
    }
    Ok(())
}
pub(super) fn assume(env: &mut BTreeMap<String, Binding>, path:&mut bounds::Clause, requirements: &[Requirement]) {
    for requirement in requirements {
        if let Predicate::Cleanup{when,required}=&requirement.predicate{*path=when.clone().negate().logical(required.clone(),true).assume(path,true);continue;}
        if let Predicate::Index(_, _, Some((scope, nonempty))) = &requirement.predicate {
            *path = scope.clone().negate().logical(nonempty.clone(), true).assume(path, true);
        }
        for (name, binding) in env.iter_mut() {
            match &requirement.predicate {
                Predicate::SameLength(left, right) => {
                    assume_length(name, binding, left, right);
                    assume_length(name, binding, right, left);
                }
                Predicate::Contiguous(_)|Predicate::Cleanup{..} => {}
                Predicate::Nonzero(selection) => {
                    if selection.root == *name && let Some(value)=selection.assumed_value(binding) {
                        for component in &selection.components {value.proof.mark_nonzero(*component);}
                    }
                }
                Predicate::Nonempty(resource) => resource.assume_nonempty(name, binding),
                Predicate::Index(selection, resource, scope) => {
                    if selection.root == *name && let Some(value) = selection.assumed_value(binding) {
                        for component in &selection.components {
                            value.proof.mark_domain(*component, resource.domain());
                        }
                    }
                    if scope.is_none() { resource.assume_nonempty(name, binding); }
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

// The selected values are the original input snapshots and owned metadata.
// Expanding a contract neither calls F nor moves or releases its argument.
fn cleanup_selection(builder:&mut Builder<'_>,definition:&Definition,env:&BTreeMap<String,Binding>,path:&str,span:Span)->Result<(returns::Returned,bounds::Condition)> {
    use returns::Returned;
    let end=path.find(['.','[']).unwrap_or(path.len());let root=&path[..end];
    if !definition.function.params.iter().any(|p|p.name==root&&matches!(p.access,Access::Value|Access::Read|Access::InOut)){return definition.source.fail(span,"B225","cleanup需要读取实际普通CPU输入");}
    let mut value=match env.get(root) {
        Some(Binding::OwnedValue(value))=>Returned::OwnedValue(value.clone()),
        Some(Binding::ApplicationData(data))=>Returned::Dataset(data.clone()),
        Some(Binding::DataReference(reference))=>{
            let mut data=reference.data.clone();let mut model=(*data.elements).clone();
            for field in &reference.projection {let Returned::OwnedValue(owner)=model else{return definition.source.fail(span,"B225","cleanup字段投影需要实际记录模型")};model=builder.unpack_record(&owner,*field);}
            data.ty=reference.element_layout().1.clone();data.elements=Box::new(model);Returned::Dataset(data)
        },
        Some(Binding::Value(value))=>Returned::value(value.clone()),
        _=>return definition.source.fail(span,"B225","cleanup需要实际按值输入或dataset共同元素"),
    };
    let mut active=bounds::Condition::constant(true);let mut rest=&path[end..];
    while !rest.is_empty(){
        if let Some(next)=rest.strip_prefix("[*]") {
            let Returned::Dataset(data)=value else{return definition.source.fail(span,"B225","cleanup的[*]选择实际dataset共同元素")};
            active=active.logical(if data.length.is_nonzero(){bounds::Condition::constant(true)}else{bounds::Condition::nonzero_value(&data.length)},false);value=*data.elements;rest=next;
        }else{
            let Some(next)=rest.strip_prefix('.')else{return definition.source.fail(span,"B225","cleanup需要实际字段或[*]路径")};
            let end=next.find(['.','[']).unwrap_or(next.len());let field=&next[..end];
            value=match value {
                Returned::OwnedValue(owner)=>{let Some(index)=owner.value.ty.fields.iter().position(|(name,..)|name==field)else{return definition.source.fail(span,"B225","cleanup需要实际拥有字段")};builder.unpack_record(&owner,index)},
                Returned::Value(value)=>Returned::value(builder.member_value(*value,&field.to_owned(),span)?),
                Returned::Dataset(data)if field=="length"=>Returned::value(data.length),
                _=>return definition.source.fail(span,"B225","cleanup字段需要实际输入布局"),
            };rest=&next[end..];
        }
    }
    Ok((value,active))
}

fn nonempty_condition(builder:&mut Builder<'_>,definition:&Definition,env:&BTreeMap<String,Binding>,resource:&Resource,span:Span)->Result<bounds::Condition>{
    let proof=match resource {
        Resource::Array(count)=>return Ok(bounds::Condition::constant(*count!=0)),
        Resource::Value(selection)=>{
            let component=if selection.value.ty.dataset.is_some(){1}else{2};
            let value=builder.value_component(&selection.value,component);
            return Ok(if value.is_nonzero(){bounds::Condition::constant(true)}else{bounds::Condition::nonzero_value(&value)});
        },
        Resource::Application{root,..}|Resource::Data{root,..}|Resource::Tile{root,..}=>length(definition,env,root,span)?.proof,
    };
    Ok(bounds::Condition::nonzero_proof(&proof,0))
}

fn requirement_condition(builder:&mut Builder<'_>,definition:&Definition,env:&BTreeMap<String,Binding>,requirement:&Requirement,span:Span)->Result<bounds::Condition>{
    if holds(builder,requirement,span)?{return Ok(bounds::Condition::constant(true));}
    Ok(match &requirement.predicate {
        Predicate::Nonzero(selection)=>bounds::Condition::nonzero_value(&selection.value),
        Predicate::Nonempty(resource)=>nonempty_condition(builder,definition,env,resource,span)?,
        Predicate::Index(selection,resource,scope)=>{
            let nonempty = match scope { Some((_,condition)) => condition.clone(), None => nonempty_condition(builder,definition,env,resource,span)? };
            let condition = bounds::Condition::index_value(&selection.value,resource.domain()).logical(nonempty,false);
            match scope { Some((exists,_)) => exists.clone().negate().logical(condition,true), None => condition }
        },
        Predicate::SameLength(left,right)=>bounds::Condition::same_length_proofs(&left.proof,&right.proof),
        Predicate::Contiguous(_)=>bounds::Condition::default(),
        Predicate::Cleanup{when,required}=>when.clone().negate().logical(required.clone(),true),
    })
}

fn cleanup_condition(builder:&mut Builder<'_>,value:returns::Returned,active:bounds::Condition,span:Span)->Result<bounds::Condition>{
    use returns::Returned;
    let mut result=bounds::Condition::constant(true);
    match value {
        Returned::OwnedValue(owner)if owner.value.ty.cleanup.is_some()=>{
            let key=owner.value.ty.cleanup.as_ref().unwrap();let definition=&builder.world.functions[key];
            let instance=generics::specialize(builder.world,key,&[generics::Actual::value(&owner.value.ty.arguments[0])],None,(&definition.source,&definition.module,&definition.imports),span)?;
            let payload=builder.unpack_record(&owner,0);let binding=match payload{Returned::Dataset(data)=>Binding::ApplicationData(data),Returned::OwnedValue(value)=>Binding::OwnedValue(value),Returned::Value(value)=>Binding::Value(*value),Returned::Unit=>Binding::Value(builder.returned_storage(&Returned::Unit))};
            let env=BTreeMap::from([(instance.definition.function.params[0].name.clone(),binding)]);
            for requirement in resolve(builder,&instance.definition,&env)? {
                result=result.logical(requirement_condition(builder,&instance.definition,&env,&requirement,span)?,false);
            }
            result=active.negate().logical(result,true);
        },
        Returned::OwnedValue(owner)=>{
            for index in 0..owner.payloads.len(){
                if owner.payloads[index].is_none(){continue;}
                let(value,when)=if owner.value.ty.has_variants(){
                    let tag=builder.value_component(&owner.value,0);let when=bounds::Condition::selected(ownership::ApplicationIndex::of(&tag),owner.value.ty.variants[index].discriminant as usize);
                    (builder.unpack_variant(&owner,index),active.clone().logical(when,false))
                }else{(builder.unpack_record(&owner,index),active.clone())};
                result=result.logical(cleanup_condition(builder,value,when,span)?,false);
            }
        },
        Returned::Dataset(data)=>{
            let when=if data.length.is_nonzero(){bounds::Condition::constant(true)}else{bounds::Condition::nonzero_value(&data.length)};
            result=cleanup_condition(builder,*data.elements,active.logical(when,false),span)?;
        },
        Returned::Value(_)|Returned::Unit=>{},
    }
    Ok(result)
}
