use super::*;

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum Resource { Done, Buffer, Acceleration, Texture, Keep }

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(super) enum Parameter { Acceleration, Texture, Dataset, Tile, Ordinary }

#[allow(dead_code)]
#[repr(u32)]
pub(super) enum DomainParameter {
    Accept,
    Reject,
    PlainLayout,
}

#[allow(dead_code)]
#[repr(u32)]
enum DomainAccessError {
    None,
    Texture,
    Acceleration,
    Dataset,
    Uniform,
}

#[repr(C)]
pub(super) struct DomainBinding {
    pub form: Parameter,
    access_error: DomainAccessError,
    pub view_check: u32,
}

impl DomainBinding {
    pub(super) fn check_access(&self, source: &Source, span: Span) -> Result<()> {
        let (code, message) = match self.access_error {
            DomainAccessError::None => return Ok(()),
            DomainAccessError::Texture => ("B239", "texture2d／texture3d 必须声明 read 资源权限"),
            DomainAccessError::Acceleration => ("B182", "acceleration 必须是 GPU program 的 read 资源"),
            DomainAccessError::Dataset => ("B101", "dataset 必须声明 read、write、inout 或 atomic"),
            DomainAccessError::Uniform => ("B102", "标量参数按值传递；访问权限只用于 dataset"),
        };
        source.fail(span, code, message)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum Dataset { Device, Owned, Borrowed }

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum ValueForm { OwnedRead, AtomicParameter, Reference, Value }

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum PlaceForm { CAbi, Native, LocalRead, LocalWrite }

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum RefBody { Atomic, OwnedInOut, OwnedOutput, ReadContent, Empty }

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum Prepare { Elements, BindElements, Resources, Resolve, Assume, Arrays, Outputs, Completions, Done }
#[derive(Clone, Copy)]
#[repr(C)]
struct Facts { dataset: u32, acceleration: u32, texture: u32, tile: u32 }

#[derive(Clone, Copy)]
#[repr(C)]
struct Mode { application: u32, c_abi: u32, cooperative: u32 }

#[derive(Clone, Copy)]
#[repr(C)]
struct Reference { initialize: u32, body: RefBody }

#[derive(Clone, Copy)]
#[repr(C)]
struct ResourceStep { need: Resource, next: u32 }
#[repr(C)]
struct UniformField {
    size: u32,
    alignment: u32,
}

struct UniformLayout<'a> {
    uniforms: &'a mut [Uniform],
    roots: &'a mut BTreeMap<String, Root>,
}

unsafe extern "C" {
    #[link_name = "bootstrap.function_inputs::resource_step"]
    fn resource_step(stage: u32, facts: Facts) -> ResourceStep;
    #[link_name = "bootstrap.function_inputs::uniform_bytes"]
    fn uniform_bytes(buffers: u32) -> u32;
    #[link_name = "bootstrap.function_inputs::layout_uniforms"]
    fn native_layout_uniforms(
        buffers: u32,
        count: usize,
        context: *mut std::ffi::c_void,
        field: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> UniformField,
        place: unsafe extern "C" fn(*mut std::ffi::c_void, usize, u32),
    ) -> u32;
    #[link_name = "bootstrap.function_inputs::parameter_form"]
    fn native_parameter_form(facts: Facts) -> Parameter;
    #[link_name = "bootstrap.function_inputs::domain_result"]
    fn native_domain_result(unit: u32, arguments: usize, dataset: u32, tile: u32, view: u32) -> bool;
    #[link_name = "bootstrap.function_inputs::domain_parameter"]
    fn native_domain_parameter(facts: Facts, access: Access) -> DomainParameter;
    #[link_name = "bootstrap.function_inputs::domain_binding"]
    fn native_domain_binding(facts: Facts, access: Access, gpu: u32, qualified: u32) -> DomainBinding;
    #[link_name = "bootstrap.function_inputs::dataset_form"]
    fn dataset_form(ordinary_cpu: u32, by_value: u32) -> Dataset;
    #[link_name = "bootstrap.function_inputs::value_form"]
    fn value_form(mode: Mode, read: u32, atomic: u32, by_value: u32, owned: u32) -> ValueForm;
    #[link_name = "bootstrap.function_inputs::place_form"]
    fn place_form(mode: Mode, read: u32) -> PlaceForm;
    #[link_name = "bootstrap.function_inputs::reference_body"]
    fn reference_body(mode: Mode, reads: u32, atomic: u32, inout: u32, owns: u32) -> Reference;
    #[link_name = "bootstrap.function_inputs::start_actions"]
    fn start_actions(mode: Mode) -> u32;
    #[link_name = "bootstrap.function_inputs::prepare"]
    fn prepare(stage: u32, mode: Mode) -> Prepare;
    #[link_name = "bootstrap.function_inputs::return_task_layout"]
    fn return_task_layout(application_entry: u32, c_abi: u32, has_result: u32) -> u32;
}

unsafe extern "C" fn uniform_field(context: *const std::ffi::c_void, index: usize) -> UniformField {
    let layout = unsafe { &*context.cast::<UniformLayout<'_>>() };
    let ty = &layout.uniforms[index].ty;
    UniformField { size: ty.size, alignment: ty.align }
}

unsafe extern "C" fn place_uniform(context: *mut std::ffi::c_void, index: usize, offset: u32) {
    let layout = unsafe { &mut *context.cast::<UniformLayout<'_>>() };
    let uniform = &mut layout.uniforms[index];
    uniform.offset = offset;
    layout.roots.insert(uniform.name.clone(), Root::Uniform(index));
}

pub(super) fn layout_uniforms(
    buffers: u32,
    uniforms: &mut [Uniform],
    roots: &mut BTreeMap<String, Root>,
) -> u32 {
    let count = uniforms.len();
    let mut layout = UniformLayout { uniforms, roots };
    unsafe {
        native_layout_uniforms(buffers, count, std::ptr::from_mut(&mut layout).cast(), uniform_field, place_uniform)
    }
}

pub(super) fn domain_result(ty: &TypeName) -> bool {
    unsafe {
        native_domain_result(
            u32::from(ty.name == "void"), ty.arguments.len(), u32::from(ty.dataset),
            u32::from(ty.tile), u32::from(ty.view),
        )
    }
}

pub(super) fn parameter_form(param: &Param) -> Parameter {
    unsafe { native_parameter_form(facts(param)) }
}

pub(super) fn domain_parameter(param: &Param) -> DomainParameter {
    unsafe { native_domain_parameter(facts(param), param.access) }
}

pub(super) fn domain_binding(param: &Param, texture: bool, gpu: bool, qualified: bool) -> DomainBinding {
    unsafe {
        native_domain_binding(facts_with_texture(param, texture), param.access, u32::from(gpu), u32::from(qualified))
    }
}

fn facts(p: &Param) -> Facts {
    facts_with_texture(p, p.ty.is_texture_resource())
}

fn facts_with_texture(p: &Param, texture: bool) -> Facts {
    Facts {
        dataset: u32::from(p.ty.dataset),
        acceleration: u32::from(p.ty.is_acceleration()),
        texture: u32::from(texture),
        tile: u32::from(p.ty.tile),
    }
}

#[inline(never)]
pub(super) async fn function_program<P: std::borrow::Borrow<Param>>(
    world: &World,
    name: &str,
    parameters: impl Iterator<Item = impl std::future::Future<Output = Result<P>>>,
    context: (&Source, &str, &[String], FunctionKind),
    target: Target,
    source_origin: Option<SourceOrigin>,
    mut retain: impl FnMut(P),
) -> Result<Program> {
    let (source, module, imports, _) = context;
    let mut buffers = Vec::new();
    let mut accelerations = Vec::new();
    let mut textures = Vec::new();
    for parameter in parameters {
        let parameter = unsafe { world.call(parameter).await }?;
        let texture = parameter.borrow().ty.texture_type();
        let parameter_facts = facts_with_texture(parameter.borrow(), texture.is_some());
        let mut cursor = 0;
        loop {
            let step = unsafe { resource_step(cursor, parameter_facts) };
            cursor = step.next;
            match step.need {
                Resource::Buffer => {
                    let p = parameter.borrow();
                    let ty = unsafe { world.call(cooperative::parameter_type(world, p, context, target)).await }?;
                    let count = unsafe { world.call(world.shape(&p.ty.count, source, p.span, module, imports)).await }?;
                    buffers.push(Buffer {
                        name: p.name.clone(),
                        access: p.access,
                        stride: ir::align_up(ty.size, ty.align),
                        ty,
                        count,
                    });
                }
                Resource::Acceleration => accelerations.push(parameter.borrow().name.clone()),
                Resource::Texture => {
                    textures.push(unsafe {
                        world.call(texture::binding(world, parameter.borrow(), texture.unwrap(), (source, module, imports))).await
                    }?);
                }
                Resource::Keep => {
                    retain(parameter);
                    break;
                }
                Resource::Done => break,
            }
        }
    }
    let uniform_bytes = unsafe { uniform_bytes(buffers.len() as u32) };
    Ok(Program {
        name: name.to_owned(),
        source_origin,
        generated_origin: None,
        target,
        uniform_bytes,
        buffers,
        accelerations,
        textures,
        uniforms: Vec::new(),
        native_frame: None,
        resources: None,
        source_stages: 0,
        stages: Vec::new(),
        functions: Arc::from([]),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(),
        element_requirements: Vec::new(),
        element_guarantees: Vec::new(),
        length_requirements: Vec::new(),
        imports: BTreeMap::new(),
    })
}

fn origin(
    b: &Builder<'_>,
    definition: &Definition,
    name: &str,
    index: usize,
    p: &Param,
) -> Option<Arc<ParameterOrigin>> {
    match definition.syntax_origin {
        crate::frontend::DefinitionSyntax::Source => Some(Arc::new(ParameterOrigin::Parameter {
            index,
            name: p.name.clone(),
            instance: Arc::from(name),
            generated: false,
            site: b.source_at(p.span, SourceSyntax::Parameter),
        })),
        crate::frontend::DefinitionSyntax::CompiledInterface => None,
    }
}

fn parameter_value(ty: Ty, regs: Vec<Reg>) -> Value {
    let proof = bounds::Proof::default().view_parameters(&ty, &[], |i| ValueIdentity::Register(regs[i]));
    Value { ty, regs, proof, uniform: false, literal: false, constant: None }
}

fn content_proof(ty: &Ty, index: usize) -> bounds::Proof {
    bounds::Proof::parameter_content(index, ty.components.len())
        .view_parameters(ty, &[], |i| ValueIdentity::ParameterContent(index, i))
}

#[allow(dead_code)] // Values are returned by bootstrap.function_inputs.
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum ReferenceForm { Empty, Whole, Split }

#[repr(C)]
pub(super) struct ReferenceLayout {
    pub(super) form: ReferenceForm,
    pub(super) count: usize,
}

#[repr(C)]
struct ReferenceFacts {
    array: u32,
    record: u32,
    view: u32,
    dataset: u32,
    variant: u32,
    enumeration: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.function_inputs::reference_layout"]
    fn b_reference_layout(facts: ReferenceFacts, components: usize, c_abi: u32) -> ReferenceLayout;
}

pub(super) fn reference_layout(ty: &Ty, c_abi: bool) -> ReferenceLayout {
    let facts = ReferenceFacts {
        array: u32::from(ty.array.is_some()),
        record: u32::from(!ty.fields.is_empty()),
        view: u32::from(ty.view.is_some()),
        dataset: u32::from(ty.dataset.is_some()),
        variant: u32::from(ty.has_variants()),
        enumeration: u32::from(ty.enumeration),
    };
    unsafe { b_reference_layout(facts, ty.components.len(), u32::from(c_abi)) }
}

fn reference_place(
    b: &mut Builder<'_>,
    mode: Mode,
    index: usize,
    p: &Param,
    ty: Ty,
    origin: Option<Arc<ParameterOrigin>>,
    parameters: &mut Vec<CallParameter>,
    inputs: &mut Vec<Reg>,
) -> Place {
    let form = unsafe { place_form(mode, u32::from(p.access == Access::Read)) };
    match form {
        PlaceForm::LocalRead | PlaceForm::LocalWrite => {
            let (slots, storage) = b.local_slots(&ty);
            match form {
                PlaceForm::LocalRead => Place::ReadOnlyLocal(ty, slots, storage),
                _ => Place::Local(ty, slots, storage),
            }
        }
        PlaceForm::CAbi | PlaceForm::Native => {
            let layout = reference_layout(&ty, matches!(form, PlaceForm::CAbi));
            let mut emit = |part, origin| {
                let address = [b.reg(Scalar::U32), b.reg(Scalar::U32)];
                inputs.extend(address);
                parameters.push(CallParameter::reference(part, p.access).with_origin(origin));
                address
            };
            let address = match layout.form {
                ReferenceForm::Empty => ReferenceAddress::Components(BTreeMap::new()),
                ReferenceForm::Whole => ReferenceAddress::Contiguous(emit(ty.clone(), origin)),
                ReferenceForm::Split => ReferenceAddress::Components(
                    ty.components.iter().map(|&(scalar, offset)| {
                        (offset, emit(Ty::scalar(scalar), origin.clone()))
                    }).collect()
                ),
            };
            Place::Reference {
                words: ty.components.iter().map(|(_, offset)| *offset).collect(),
                ty,
                address,
                scope: Some(index),
                access: p.access,
            }
        }
    }
}

async fn ordinary_parameter(
    b: &mut Builder<'_>,
    definition: &Definition,
    name: &str,
    mode: Mode,
    index: usize,
    p: Param,
    ty: Ty,
    parameters: &mut Vec<CallParameter>,
    inputs: &mut Vec<Reg>,
) -> Result<()> {
    let owned = ty.owns_resources();
    let form = unsafe {
        value_form(
            mode, u32::from(p.access == Access::Read), u32::from(p.access == Access::Atomic),
            u32::from(p.access == Access::Value), u32::from(owned),
        )
    };
    let binding = match form {
        ValueForm::OwnedRead | ValueForm::Value => {
            let regs: Vec<_> = ty.components.iter().map(|(scalar, _)| b.reg(*scalar)).collect();
            if matches!(form, ValueForm::OwnedRead) || mode.application != 0 {
                inputs.extend(&regs);
                parameters.push(CallParameter::value(ty.clone()).with_origin(origin(b, definition, name, index, &p)));
            }
            let value = parameter_value(ty, regs);
            if owned {
                let mut value = unsafe { b.world.call(b.incoming_owned_value(value)).await }?;
                if matches!(form, ValueForm::OwnedRead) {
                    value.borrow();
                }
                residency::parameter(&mut value, index);
                b.bind_owned_value(&p.name, value, p.span)?;
                return Ok(());
            }
            Binding::Value(value)
        }
        ValueForm::AtomicParameter => Binding::AtomicBorrow(Place::AtomicParameter(ty)),
        ValueForm::Reference => {
            let origin = if mode.application != 0 { origin(b, definition, name, index, &p) } else { None };
            let place = reference_place(b, mode, index, &p, ty, origin, parameters, inputs);
            let decision = unsafe {
                reference_body(
                    mode, u32::from(p.access.reads()), u32::from(p.access == Access::Atomic),
                    u32::from(p.access == Access::InOut), u32::from(owned),
                )
            };
            if decision.initialize != 0 {
                b.initialize(&place);
            }
            match decision.body {
                RefBody::Atomic => Binding::AtomicBorrow(place),
                RefBody::OwnedInOut | RefBody::OwnedOutput => {
                    let ty = place.ty().clone();
                    let regs = ty.components.iter().map(|(scalar, _)| b.reg(*scalar)).collect();
                    let inout = matches!(decision.body, RefBody::OwnedInOut);
                    let proof = if inout { content_proof(&ty, index) } else { bounds::Proof::default() };
                    let value = Value { ty, regs, proof, uniform: false, literal: false, constant: None };
                    let returns::Returned::OwnedValue(mut value) =
                        unsafe { b.world.call(b.template_returned(value, inout)).await }? else { unreachable!() };
                    if inout {
                        residency::parameter(&mut value, index);
                    }
                    value.place = Some(place);
                    Binding::OwnedValue(value)
                }
                RefBody::ReadContent | RefBody::Empty => {
                    let proof = if matches!(decision.body, RefBody::ReadContent) {
                        content_proof(place.ty(), index)
                    } else {
                        bounds::Proof::default()
                    };
                    Binding::Owned(OwnedBinding { proof, place, uniform: false, initialization: None })
                }
            }
        }
    };
    b.env.insert(p.name, binding);
    Ok(())
}

async fn dataset_parameter(
    b: &mut Builder<'_>,
    definition: &Definition,
    name: &str,
    index: usize,
    data_index: usize,
    p: &Param,
    parameters: &mut Vec<CallParameter>,
    inputs: &mut Vec<Reg>,
) -> Result<Binding> {
    let ordinary_cpu = definition.function.kind() == FunctionKind::Value && b.program.target == Target::Cpu;
    let role = unsafe { dataset_form(u32::from(ordinary_cpu), u32::from(p.access == Access::Value)) };
    match role {
        Dataset::Device => {
            b.view_access.insert(data_index, p.access);
            if p.access == Access::InOut {
                b.initialized.extend(
                    b.program.buffers[data_index].ty.components.iter()
                        .map(|(_, word)| Position::Element(data_index, *word)),
                );
            }
            Ok(Binding::Data(b.dataset_view(data_index, p.access, true)))
        }
        Dataset::Owned => {
            let data = unsafe { b.world.call(b.application_parameter(data_index, index, p.access, Scalar::U32)).await }?;
            parameters.push(CallParameter::value(Ty::owned_dataset(data.ty.syntax(), data.count))
                .with_origin(origin(b, definition, name, index, p)));
            inputs.extend([data.root.regs[0], data.length.regs[0]]);
            let rebound = application::rebound(definition.function.body().as_ref().unwrap(), &p.name);
            Ok(Binding::ApplicationData(b.application_bind(data, rebound)))
        }
        Dataset::Borrowed => {
            let ty = Builder::borrowed_dataset_parameter_type(p.access, definition.execution);
            let root_scalar = ty.components[0].0;
            let stride_scalar = ty.components[2].0;
            let mut data =
                unsafe { b.world.call(b.application_parameter(data_index, index, p.access, root_scalar)).await }?;
            let stride = b.reg(stride_scalar);
            let offset = b.reg(Scalar::U32);
            let layout_value = |reg, scalar| Value {
                ty: Ty::scalar(scalar), regs: vec![reg], proof: bounds::Proof::default(),
                literal: false, uniform: false, constant: None,
            };
            parameters.push(CallParameter::value(ty).with_origin(origin(b, definition, name, index, p)));
            inputs.extend([data.root.regs[0], data.length.regs[0], stride, offset]);
            data.layout = application::DataLayout::Parameter {
                parameter: index, stride: layout_value(stride, stride_scalar), offset: layout_value(offset, Scalar::U32),
            };
            Ok(Binding::ApplicationData(data))
        }
    }
}

fn completion_inputs(
    b: &mut Builder<'_>,
    span: Span,
    parameters: &mut Vec<CallParameter>,
    inputs: &mut Vec<Reg>,
) -> Result<()> {
    let first_completion = inputs.len();
    let (_, completions) = native_function::task_completion_parameters(
        parameters.iter().map(|parameter| match &parameter.kind {
            CallParameterKind::Value(ty) => (ty.components.len(), Some(ty)),
            CallParameterKind::Reference { .. } => (2, None),
        }),
        0,
    );
    for completion in completions {
        let source = match completion.parent() {
            Some((parent, component)) => TaskCompletionSource::Returned(inputs[first_completion + 2 * parent], component),
            None => TaskCompletionSource::Input(inputs[completion.input()]),
        };
        let address = [b.reg(Scalar::U32), b.reg(Scalar::U32)];
        let layout = b.task_completion_layout(&completion.result, span)?;
        b.task_completions.entry(source).or_default().push((completion.result, address));
        parameters.push(CallParameter::reference(layout, Access::InOut));
        inputs.extend(address);
    }
    Ok(())
}

pub(super) fn return_inputs(
    b: &mut Builder<'_>,
    c_abi: bool,
    result: Option<&Ty>,
    parameters: &mut Vec<CallParameter>,
    inputs: &mut Vec<Reg>,
) {
    let needed = unsafe {
        return_task_layout(u32::from(b.application_entry()), u32::from(c_abi), u32::from(result.is_some()))
    };
    if needed != 0 {
        let task_results = result.unwrap().task_components();
        if !task_results.is_empty() {
            let address = [b.reg(Scalar::U32), b.reg(Scalar::U32)];
            b.task_return_source = Some(address);
            parameters.push(CallParameter::reference(Ty::task_return_layout(&task_results), Access::Write));
            inputs.extend(address);
        }
    }
}

#[inline(never)]
pub(super) async fn function_parameters(
    b: &mut Builder<'_>,
    instance: &crate::generics::Instance,
    name: &str,
    bound_parameters: Vec<Param>,
) -> Result<(Vec<CallParameter>, Vec<Reg>, BTreeSet<(ContractKind, ContractPredicate)>)> {
    let world = b.world;
    let d = &instance.definition;
    let mut inputs = Vec::new();
    let mut parameters = Vec::new();
    let mode = Mode {
        application: u32::from(b.application.is_some()),
        c_abi: u32::from(d.function.c_abi()),
        cooperative: u32::from(d.function.kind() == FunctionKind::Cooperative),
    };
    let start = unsafe { start_actions(mode) };
    if start & 1 != 0 {
        let frame = b.application_frame(d.function.span())?;
        parameters.push(CallParameter::value(frame.ty).with_origin(Some(Arc::new(ParameterOrigin::Frame))));
        inputs.extend(frame.regs);
    }
    b.calls.push(name.into());
    if start & 2 != 0 {
        b.cooperative_root = Some(0);
    }
    let mut data_index = 0;
    let mut texture_index = 0;
    let mut acceleration_index = 0;
    let context: (&Source, &str, &[String], FunctionKind) = (&d.source, &d.module, &d.imports, d.function.kind());
    for (index, p) in bound_parameters.into_iter().enumerate() {
        let form = parameter_form(&p);
        let binding = match form {
            Parameter::Acceleration => {
                let binding = Binding::Acceleration(acceleration_index);
                acceleration_index += 1;
                binding
            }
            Parameter::Texture => {
                let address = b.reg(Scalar::U64);
                let origin = origin(b, d, name, index, &p);
                parameters.push(CallParameter::value(Ty::scalar(Scalar::U64)).with_origin(origin));
                inputs.push(address);
                let mut resource = texture::TextureReference::binding(texture_index);
                resource.source.parameter = Some(address);
                texture_index += 1;
                Binding::Texture(resource)
            }
            Parameter::Dataset => {
                let binding = unsafe {
                    world.call(dataset_parameter(b, d, name, index, data_index, &p, &mut parameters, &mut inputs)).await
                }?;
                data_index += 1;
                binding
            }
            Parameter::Tile | Parameter::Ordinary => {
                let ty = unsafe { world.call(cooperative::parameter_type(world, &p, context, b.program.target)).await }?;
                if matches!(form, Parameter::Tile) {
                    let tile = b.tiles.len();
                    b.tiles.push(CheckedTile {
                        source: TileOrigin::Parameter(ty),
                        width: TileWidth::Parameter,
                        access: p.access,
                    });
                    b.bind_tile(&p.name, tile);
                } else {
                    unsafe { world.call(ordinary_parameter(b, d, name, mode, index, p, ty, &mut parameters, &mut inputs)).await }?;
                }
                continue;
            }
        };
        b.env.insert(p.name, binding);
    }
    let mut paths = None;
    let mut resources = BTreeMap::new();
    let mut requirements = None;
    let mut output_contracts = BTreeSet::new();
    let mut phase = 0;
    loop {
        let action = unsafe { prepare(phase, mode) };
        phase += 1;
        match action {
            Prepare::Elements => {
                let mut env = std::mem::take(&mut b.env);
                paths = Some(unsafe { world.call(contracts::element_input_paths(b, instance.into(), &mut env, None)).await }?);
                b.env = env;
            }
            Prepare::BindElements => {
                application::bind_parameter_elements(b, instance.into(), &paths.take().unwrap());
            }
            Prepare::Resources => {
                resources = d.function.params().iter().enumerate().filter_map(|(index, p)| match b.env.get(p.name()) {
                    Some(Binding::ApplicationData(data)) => {
                        let mut proof = bounds::Proof::default();
                        proof.set_view_borrow(0, view::resource_borrow(data));
                        Some((ValueIdentity::ResourceParameter(index, 0), proof))
                    }
                    _ => None,
                }).collect();
            }
            Prepare::Resolve => {
                let env = std::mem::take(&mut b.env);
                let resolved = unsafe {
                    world.call(contracts::resolve(b, instance.into(), &LexicalScope::Function(instance.clone()), &env, &resources)).await
                };
                b.env = env;
                requirements = Some(resolved?);
            }
            Prepare::Assume => {
                let mut env = std::mem::take(&mut b.env);
                unsafe { world.call(contracts::assume(b, instance.into(), &mut env, &requirements.take().unwrap())).await }?;
                b.env = env;
            }
            Prepare::Arrays => b.materialize_array_bindings(),
            Prepare::Outputs => output_contracts = unsafe { world.call(outputs::contract_predicates(b, instance)).await }?,
            Prepare::Completions => completion_inputs(b, d.function.span(), &mut parameters, &mut inputs)?,
            Prepare::Done => return Ok((parameters, inputs, output_contracts)),
        }
    }
}
