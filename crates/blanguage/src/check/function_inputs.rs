use super::*;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(super) enum Parameter { Acceleration, Texture, Dataset, Tile, Ordinary }

#[allow(dead_code)]
#[repr(u32)]
pub(super) enum DomainLayout {
    Accept,
    Reject,
    PlainLayout,
}

#[repr(C)]
pub(super) struct DomainParameter {
    pub form: Parameter,
    pub layout: DomainLayout,
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
    access_error: DomainAccessError,
    pub view_check: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Facts { dataset: u32, acceleration: u32, texture: u32, tile: u32 }

#[derive(Clone, Copy)]
#[repr(C)]
struct Mode { application: u32, c_abi: u32, cooperative: u32 }

#[derive(Clone,Copy,Default)]#[repr(C)]struct ProgramCursor {phase:u32,resource:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct ProgramFacts {present:u32,shape:Facts,buffers:u32}
#[allow(dead_code)]#[repr(u32)]enum ProgramNeed {Next,Buffer,Acceleration,Texture,Keep,Finish}
#[repr(C)]struct ProgramStep {cursor:ProgramCursor,need:ProgramNeed,uniform_bytes:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct BindingCursor {index:u64,data:u64,texture:u64,acceleration:u64,phase:u32,prepare:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct BindingFacts {present:u32,shape:Facts}
#[allow(dead_code)]#[repr(u32)]enum BindingNeed {Frame,Calls,Cooperative,Next,Acceleration,Texture,Dataset,Type,Tile,Ordinary,Bind,Elements,BindElements,Resources,Resolve,Assume,Arrays,Outputs,Completions,Finish}
#[repr(C)]struct BindingStep {cursor:BindingCursor,need:BindingNeed}
#[derive(Clone,Copy,Default)]#[repr(C)]struct InputCursor {phase:u32,index:u64}
#[derive(Clone,Copy,Default)]#[repr(C)]struct DatasetFacts {ordinary_cpu:u32,by_value:u32,inout:u32,components:u64}
#[allow(dead_code)]#[repr(u32)]enum DatasetNeed {Access,Components,Initialize,Device,Application,OwnedAbi,Rebound,Owned,Layout,Registers,BorrowedAbi,Borrowed}
#[repr(C)]struct DatasetStep {cursor:InputCursor,need:DatasetNeed}
#[derive(Clone,Copy,Default)]#[repr(C)]struct OrdinaryFacts {owned:u32,components:u64,reference:ReferenceFacts}
#[allow(dead_code)]#[repr(u32)]enum OrdinaryNeed {Components,Register,ValueAbi,Value,Incoming,Borrow,Residency,OwnedValue,PlainValue,AtomicParameter,Origin,LocalRead,LocalWrite,ReferenceFacts,ReferenceWhole,ReferenceSplit,ReferenceWord,ReferencePlace,Initialize,AtomicReference,ReferenceType,ContentProof,EmptyProof,TemplateValue,TemplateReturned,OwnedReference,Reference}
#[repr(C)]struct OrdinaryStep {cursor:InputCursor,need:OrdinaryNeed,form:ReferenceForm,inout:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct CompletionFacts {present:u32,nested:u32,parent:u64,component:u64,input:u64}
#[allow(dead_code)]#[repr(u32)]enum CompletionNeed {Next,Publish,Finish}
#[allow(dead_code)]#[repr(u32)]enum CompletionSource {Input,Returned}
#[repr(C)]struct CompletionStep {cursor:InputCursor,need:CompletionNeed,source:CompletionSource,input:u64,component:u64}
#[derive(Clone,Copy,Default)]#[repr(C)]struct ReturnFacts {application:u32,c_abi:u32,result:u32,nonempty:u32}
#[allow(dead_code)]#[repr(u32)]enum ReturnNeed {Inspect,Publish,Finish}
#[repr(C)]struct ReturnStep {cursor:InputCursor,need:ReturnNeed}

#[repr(C)]
struct UniformField {
    size: u32,
    alignment: u32,
}

#[repr(C)]
pub(super) struct DomainBytes {
    pub bytes: u32,
    pub exceeded: bool,
}

#[repr(C)]
struct DomainInputFields {
    shape: Facts,
    form: Parameter,
    access: Access,
    bound: u32,
    layout: u32,
}

#[allow(dead_code)]
#[repr(u32)]
enum DomainPlace { Texture, Acceleration, Uniform }

#[allow(dead_code)]
#[repr(u32)]
enum DomainQuery { Owns, Views, Size, Alignment }

#[allow(dead_code)]
#[derive(PartialEq, Eq)]
#[repr(u32)]
enum DomainWorkKind {
    Start, Resolve, Texture, Dataset, Done, Owned, Views,
    TextureAccess, AccelerationAccess, DatasetAccess, UniformAccess,
}

#[repr(C)]
struct DomainWork {
    index: usize,
    kind: DomainWorkKind,
    binding: DomainBinding,
    form: Parameter,
    stride: u32,
}

#[repr(C)]
struct DomainParameterHost {
    context: *mut std::ffi::c_void,
    inspect: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> DomainInputFields,
    query: unsafe extern "C" fn(*const std::ffi::c_void, DomainQuery) -> u32,
    place: unsafe extern "C" fn(*mut std::ffi::c_void, usize, DomainPlace),
}

struct DomainParameters<'a, 'b> {
    parameters: &'a [Param],
    bindings: DomainBindings<'a>,
    layout: Option<Ty>,
    program: &'b mut Program,
    roots: &'b mut BTreeMap<String, Root>,
    entry: Option<&'b mut native_function::DomainEntry>,
}

struct UniformLayout<'a> {
    uniforms: &'a mut [Uniform],
    roots: &'a mut BTreeMap<String, Root>,
}

unsafe extern "C" {
    #[link_name = "bootstrap.function_inputs::program_step"]
    fn program_step(cursor:ProgramCursor,facts:ProgramFacts)->ProgramStep;
    #[link_name = "bootstrap.function_inputs::layout_uniforms"]
    fn native_layout_uniforms(
        buffers: u32,
        count: usize,
        limit: u32,
        context: *mut std::ffi::c_void,
        field: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> UniformField,
        place: unsafe extern "C" fn(*mut std::ffi::c_void, usize, u32),
    ) -> DomainBytes;
    #[link_name = "bootstrap.function_inputs::domain_parameters"]
    fn native_domain_parameters(work: DomainWork, count: usize, gpu: u32, host: DomainParameterHost) -> DomainWork;
    #[link_name = "bootstrap.function_inputs::parameter_form"]
    fn native_parameter_form(facts: Facts) -> Parameter;
    #[link_name = "bootstrap.function_inputs::domain_result"]
    fn native_domain_result(unit: u32, arguments: usize, dataset: u32, tile: u32, view: u32) -> bool;
    #[link_name = "bootstrap.function_inputs::domain_parameter"]
    fn native_domain_parameter(facts: Facts, access: Access) -> DomainParameter;
    #[link_name = "bootstrap.function_inputs::dataset_step"]
    fn dataset_step(cursor:InputCursor,facts:DatasetFacts)->DatasetStep;
    #[link_name = "bootstrap.function_inputs::ordinary_step"]
    fn ordinary_step(cursor:InputCursor,mode:Mode,access:Access,facts:OrdinaryFacts)->OrdinaryStep;
    #[link_name = "bootstrap.function_inputs::binding_step"]
    fn binding_step(cursor:BindingCursor,mode:Mode,facts:BindingFacts)->BindingStep;
    #[link_name = "bootstrap.function_inputs::resource_parameters"]
    fn resource_parameters(count:u64,context:*mut std::ffi::c_void,inspect:unsafe extern "C" fn(*mut std::ffi::c_void,u64)->u32,publish:unsafe extern "C" fn(*mut std::ffi::c_void,u64));
    #[link_name = "bootstrap.function_inputs::completion_step"]
    fn completion_step(cursor:InputCursor,first:u64,facts:CompletionFacts)->CompletionStep;
    #[link_name = "bootstrap.function_inputs::return_step"]
    fn return_step(cursor:InputCursor,facts:ReturnFacts)->ReturnStep;
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
    limit: u32,
) -> DomainBytes {
    let count = uniforms.len();
    let mut layout = UniformLayout { uniforms, roots };
    unsafe {
        native_layout_uniforms(buffers, count, limit, std::ptr::from_mut(&mut layout).cast(), uniform_field, place_uniform)
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

unsafe extern "C" fn inspect_domain_parameter(context: *mut std::ffi::c_void, index: usize) -> DomainInputFields {
    let inputs = unsafe { &mut *context.cast::<DomainParameters<'_, '_>>() };
    let parameter = &inputs.parameters[index];
    match inputs.bindings.next().flatten() {
        Some(binding) => {
            inputs.layout = binding.layout;
            DomainInputFields {
                shape: Facts::default(), form: binding.form, access: parameter.access,
                bound: 1, layout: u32::from(inputs.layout.is_some()),
            }
        }
        None => DomainInputFields {
            shape: facts(parameter), form: Parameter::Ordinary, access: parameter.access,
            bound: 0, layout: 0,
        },
    }
}

unsafe extern "C" fn query_domain_layout(context: *const std::ffi::c_void, query: DomainQuery) -> u32 {
    let inputs = unsafe { &*context.cast::<DomainParameters<'_, '_>>() };
    let ty = inputs.layout.as_ref().unwrap();
    match query {
        DomainQuery::Owns => u32::from(ty.owns_resources()),
        DomainQuery::Views => u32::from(ty.contains_views()),
        DomainQuery::Size => ty.size,
        DomainQuery::Alignment => ty.align,
    }
}

unsafe extern "C" fn place_domain_parameter(context: *mut std::ffi::c_void, index: usize, place: DomainPlace) {
    let inputs = unsafe { &mut *context.cast::<DomainParameters<'_, '_>>() };
    let parameter = &inputs.parameters[index];
    match place {
        DomainPlace::Texture => {
            inputs.roots.insert(parameter.name.clone(), Root::Texture(inputs.program.textures.len()));
        }
        DomainPlace::Acceleration => {
            inputs.roots.insert(parameter.name.clone(), Root::Acceleration(inputs.program.accelerations.len()));
            inputs.program.accelerations.push(parameter.name.clone());
        }
        DomainPlace::Uniform => {
            let ty = inputs.layout.take().unwrap();
            if let Some(entry) = inputs.entry.as_deref_mut() { entry.uniform(&ty); }
            inputs.program.uniforms.push(Uniform { name: parameter.name.clone(), ty, offset: 0 });
        }
    }
}

#[inline(never)]
pub(super) async fn domain_parameters(
    world: &World,
    definition: &Definition,
    parameters: &[Param],
    bindings: DomainBindings<'_>,
    program: &mut Program,
    roots: &mut BTreeMap<String, Root>,
    entry: Option<&mut native_function::DomainEntry>,
) -> Result<()> {
    let count = match &bindings {
        DomainBindings::Owned(bindings) => parameters.len().min(bindings.len()),
        DomainBindings::Shared(bindings) => parameters.len().min(bindings.len()),
        DomainBindings::Unqualified => parameters.len(),
    };
    let gpu = u32::from(program.target == Target::Gpu);
    let mut inputs = DomainParameters { parameters, bindings, layout: None, program, roots, entry };
    let mut work = DomainWork {
        index: 0, kind: DomainWorkKind::Start, form: Parameter::Ordinary, stride: 0,
        binding: DomainBinding { access_error: DomainAccessError::None, view_check: 0 },
    };
    let (source, module, imports) = (&definition.source, &definition.module, &definition.imports);
    loop {
        let host = DomainParameterHost {
            context: std::ptr::from_mut(&mut inputs).cast(),
            inspect: inspect_domain_parameter, query: query_domain_layout, place: place_domain_parameter,
        };
        work = unsafe { native_domain_parameters(work, count, gpu, host) };
        let (code, message) = match work.kind {
            DomainWorkKind::Resolve => {
                let parameter = &parameters[work.index];
                inputs.layout = Some(unsafe {
                    world.call(world.ty(&parameter.ty, source, parameter.span, module, imports)).await
                }?);
                continue;
            }
            DomainWorkKind::Texture => {
                let parameter = &parameters[work.index];
                inputs.program.textures.push(unsafe {
                    world.call(texture::binding(world, parameter, parameter.ty.texture_type().unwrap(), (source, module, imports))).await
                }?);
                if let Some(entry) = inputs.entry.as_deref_mut() { entry.texture(); }
                continue;
            }
            DomainWorkKind::Dataset => {
                let parameter = &parameters[work.index];
                let binding = inputs.program.buffers.len();
                let ty = inputs.layout.take().unwrap();
                let count = unsafe { world.call(world.shape(&parameter.ty.count, source, parameter.span, module, imports)).await }?;
                inputs.program.buffers.push(Buffer {
                    name: parameter.name.clone(), access: parameter.access, ty, stride: work.stride, count,
                });
                inputs.roots.insert(parameter.name.clone(), Root::Data(binding, parameter.access));
                if let Some(entry) = inputs.entry.as_deref_mut() { entry.dataset(); }
                continue;
            }
            DomainWorkKind::Owned => ("B224", "拥有结果属于CPU资源组织，不能成为program的普通存储或uniform"),
            DomainWorkKind::Views => ("B185", "含view的整域根需要read／inout dataset；write／atomic或uniform不能构造引用图"),
            DomainWorkKind::TextureAccess => ("B239", "texture2d／texture3d 必须声明 read 资源权限"),
            DomainWorkKind::AccelerationAccess => ("B182", "acceleration 必须是 GPU program 的 read 资源"),
            DomainWorkKind::DatasetAccess => ("B101", "dataset 必须声明 read、write、inout 或 atomic"),
            DomainWorkKind::UniformAccess => ("B102", "标量参数按值传递；访问权限只用于 dataset"),
            DomainWorkKind::Done => return Ok(()),
            DomainWorkKind::Start => unsafe { std::hint::unreachable_unchecked() },
        };
        return source.fail(parameters[work.index].span, code, message);
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
    let mut producer=parameters;let mut parameter:Option<P>=None;
    let mut cursor=ProgramCursor::default();let mut control=ProgramFacts::default();
    let uniform_bytes=loop {
        control.buffers=buffers.len() as u32;
        let step=unsafe {program_step(cursor,control)};cursor=step.cursor;
        match step.need {
            ProgramNeed::Next=>{
                let next=producer.next();control.present=u32::from(next.is_some());
                if let Some(next)=next {
                    let value=unsafe {world.call(next).await}?;
                    let texture=value.borrow().ty.texture_type().is_some();control.shape=facts_with_texture(value.borrow(),texture);
                    parameter=Some(value);
                }
            },
            ProgramNeed::Buffer=>{
                let p:&Param=parameter.as_ref().unwrap().borrow();
                let ty=unsafe {world.call(cooperative::parameter_type(world,p,context,target)).await}?;
                let count=unsafe {world.call(world.shape(&p.ty.count,source,p.span,module,imports)).await}?;
                buffers.push(Buffer{name:p.name.clone(),access:p.access,stride:ir::align_up(ty.size,ty.align),ty,count});
            },
            ProgramNeed::Acceleration=>{let p:&Param=parameter.as_ref().unwrap().borrow();accelerations.push(p.name.clone());},
            ProgramNeed::Texture=>{
                let p:&Param=parameter.as_ref().unwrap().borrow();let texture=p.ty.texture_type().unwrap();
                textures.push(unsafe {world.call(texture::binding(world,p,texture,(source,module,imports))).await}?);
            },
            ProgramNeed::Keep=>retain(parameter.take().unwrap()),
            ProgramNeed::Finish=>break step.uniform_bytes,
        }
    };
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

#[derive(Clone,Copy,Default)]
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

async fn ordinary_parameter(
    b:&mut Builder<'_>,definition:&Definition,name:&str,mode:Mode,index:usize,p:Param,ty:Ty,
    parameters:&mut Vec<CallParameter>,inputs:&mut Vec<Reg>,
)->Result<()> {
    let mut cursor=InputCursor::default();let mut facts=OrdinaryFacts{owned:u32::from(ty.owns_resources()),..Default::default()};
    let mut ty=Some(ty);let mut regs=Vec::new();let mut value=None;let mut owned:Option<owned_values::OwnedValue>=None;
    let mut place:Option<Place>=None;let mut proof=None;let mut parameter_origin=None;
    let mut address=None;let mut component_addresses=BTreeMap::new();let mut words=Vec::new();
    loop {
        let step=unsafe{ordinary_step(cursor,mode,p.access,facts)};cursor=step.cursor;
        match step.need {
            OrdinaryNeed::Components=>{facts.components=ty.as_ref().unwrap().components.len() as u64;},
            OrdinaryNeed::Register=>{regs.push(b.reg(ty.as_ref().unwrap().components[cursor.index as usize].0));},
            OrdinaryNeed::ValueAbi=>{inputs.extend(&regs);parameters.push(CallParameter::value(ty.as_ref().unwrap().clone()).with_origin(origin(b,definition,name,index,&p)));},
            OrdinaryNeed::Value=>{value=Some(parameter_value(ty.take().unwrap(),std::mem::take(&mut regs)));},
            OrdinaryNeed::Incoming=>{owned=Some(unsafe{b.world.call(b.incoming_owned_value(value.take().unwrap())).await}?);},
            OrdinaryNeed::Borrow=>owned.as_mut().unwrap().borrow(),
            OrdinaryNeed::Residency=>residency::parameter(owned.as_mut().unwrap(),index),
            OrdinaryNeed::OwnedValue=>{b.bind_owned_value(&p.name,owned.take().unwrap(),p.span)?;return Ok(());},
            OrdinaryNeed::PlainValue=>{b.env.insert(p.name,Binding::Value(value.take().unwrap()));return Ok(());},
            OrdinaryNeed::AtomicParameter=>{b.env.insert(p.name,Binding::AtomicBorrow(Place::AtomicParameter(ty.take().unwrap())));return Ok(());},
            OrdinaryNeed::Origin=>{parameter_origin=origin(b,definition,name,index,&p);},
            OrdinaryNeed::LocalRead=>{let ty=ty.take().unwrap();let (slots,storage)=b.local_slots(&ty);place=Some(Place::ReadOnlyLocal(ty,slots,storage));},
            OrdinaryNeed::LocalWrite=>{let ty=ty.take().unwrap();let (slots,storage)=b.local_slots(&ty);place=Some(Place::Local(ty,slots,storage));},
            OrdinaryNeed::ReferenceFacts=>{
                let ty=ty.as_ref().unwrap();facts.components=ty.components.len() as u64;
                facts.reference=ReferenceFacts{array:u32::from(ty.array.is_some()),record:u32::from(!ty.fields.is_empty()),view:u32::from(ty.view.is_some()),dataset:u32::from(ty.dataset.is_some()),variant:u32::from(ty.has_variants()),enumeration:u32::from(ty.enumeration)};
            },
            OrdinaryNeed::ReferenceWhole=>{
                let at=[b.reg(Scalar::U32),b.reg(Scalar::U32)];inputs.extend(at);
                parameters.push(CallParameter::reference(ty.as_ref().unwrap().clone(),p.access).with_origin(parameter_origin.take()));address=Some(at);
            },
            OrdinaryNeed::ReferenceSplit=>{
                let (scalar,word)=ty.as_ref().unwrap().components[cursor.index as usize];let at=[b.reg(Scalar::U32),b.reg(Scalar::U32)];
                inputs.extend(at);parameters.push(CallParameter::reference(Ty::scalar(scalar),p.access).with_origin(parameter_origin.clone()));
                component_addresses.insert(word,at);words.push(word);
            },
            OrdinaryNeed::ReferenceWord=>words.push(ty.as_ref().unwrap().components[cursor.index as usize].1),
            OrdinaryNeed::ReferencePlace=>{
                let address=match step.form{ReferenceForm::Whole=>ReferenceAddress::Contiguous(address.take().unwrap()),ReferenceForm::Empty|ReferenceForm::Split=>ReferenceAddress::Components(std::mem::take(&mut component_addresses))};
                place=Some(Place::Reference{words:std::mem::take(&mut words),ty:ty.take().unwrap(),address,scope:Some(index),access:p.access});
            },
            OrdinaryNeed::Initialize=>b.initialize(place.as_ref().unwrap()),
            OrdinaryNeed::AtomicReference=>{b.env.insert(p.name,Binding::AtomicBorrow(place.take().unwrap()));return Ok(());},
            OrdinaryNeed::ReferenceType=>{ty=Some(place.as_ref().unwrap().ty().clone());},
            OrdinaryNeed::ContentProof=>{proof=Some(content_proof(place.as_ref().unwrap().ty(),index));},
            OrdinaryNeed::EmptyProof=>{proof=Some(bounds::Proof::default());},
            OrdinaryNeed::TemplateValue=>{value=Some(Value{ty:ty.take().unwrap(),regs:std::mem::take(&mut regs),proof:proof.take().unwrap(),uniform:false,literal:false,constant:None});},
            OrdinaryNeed::TemplateReturned=>{
                let returns::Returned::OwnedValue(result)=unsafe{b.world.call(b.template_returned(value.take().unwrap(),step.inout!=0)).await}? else{unreachable!()};owned=Some(result);
            },
            OrdinaryNeed::OwnedReference=>{let mut value=owned.take().unwrap();value.place=Some(place.take().unwrap());b.env.insert(p.name,Binding::OwnedValue(value));return Ok(());},
            OrdinaryNeed::Reference=>{b.env.insert(p.name,Binding::Owned(OwnedBinding{proof:proof.take().unwrap(),place:place.take().unwrap(),uniform:false,initialization:None}));return Ok(());},
        }
    }
}

async fn dataset_parameter(
    b:&mut Builder<'_>,definition:&Definition,name:&str,index:usize,data_index:usize,p:&Param,
    parameters:&mut Vec<CallParameter>,inputs:&mut Vec<Reg>,
)->Result<Binding> {
    let mut cursor=InputCursor::default();let mut facts=DatasetFacts{
        ordinary_cpu:u32::from(definition.function.kind()==FunctionKind::Value&&b.program.target==Target::Cpu),
        by_value:u32::from(p.access==Access::Value),inout:u32::from(p.access==Access::InOut),components:0,
    };
    let mut data:Option<application::Data>=None;let mut ty=None;let mut root_scalar=Scalar::U32;let mut stride_scalar=None;
    let mut stride=None;let mut offset=None;let mut rebound=false;
    loop {
        let step=unsafe{dataset_step(cursor,facts)};cursor=step.cursor;
        match step.need {
            DatasetNeed::Access=>{b.view_access.insert(data_index,p.access);},
            DatasetNeed::Components=>{facts.components=b.program.buffers[data_index].ty.components.len() as u64;},
            DatasetNeed::Initialize=>{let word=b.program.buffers[data_index].ty.components[cursor.index as usize].1;b.initialized.insert(Position::Element(data_index,word));},
            DatasetNeed::Device=>return Ok(Binding::Data(b.dataset_view(data_index,p.access,true))),
            DatasetNeed::Application=>{data=Some(unsafe{b.world.call(b.application_parameter(data_index,index,p.access,root_scalar)).await}?);},
            DatasetNeed::OwnedAbi=>{
                let data=data.as_ref().unwrap();parameters.push(CallParameter::value(Ty::owned_dataset(data.ty.syntax(),data.count)).with_origin(origin(b,definition,name,index,p)));
                inputs.extend([data.root.regs[0],data.length.regs[0]]);
            },
            DatasetNeed::Rebound=>{rebound=application::rebound(definition.function.body().as_ref().unwrap(),&p.name);},
            DatasetNeed::Owned=>return Ok(Binding::ApplicationData(b.application_bind(data.take().unwrap(),rebound))),
            DatasetNeed::Layout=>{let layout=Builder::borrowed_dataset_parameter_type(p.access,definition.execution);root_scalar=layout.components[0].0;stride_scalar=Some(layout.components[2].0);ty=Some(layout);},
            DatasetNeed::Registers=>{stride=Some(b.reg(stride_scalar.unwrap()));offset=Some(b.reg(Scalar::U32));},
            DatasetNeed::BorrowedAbi=>{
                parameters.push(CallParameter::value(ty.take().unwrap()).with_origin(origin(b,definition,name,index,p)));
                let data=data.as_mut().unwrap();let stride=stride.take().unwrap();let offset=offset.take().unwrap();
                inputs.extend([data.root.regs[0],data.length.regs[0],stride,offset]);
                let layout_value=|reg,scalar|Value{ty:Ty::scalar(scalar),regs:vec![reg],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None};
                data.layout=application::DataLayout::Parameter{parameter:index,stride:layout_value(stride,stride_scalar.unwrap()),offset:layout_value(offset,Scalar::U32)};
            },
            DatasetNeed::Borrowed=>return Ok(Binding::ApplicationData(data.take().unwrap())),
        }
    }
}

fn completion_inputs(
    b:&mut Builder<'_>,span:Span,parameters:&mut Vec<CallParameter>,inputs:&mut Vec<Reg>,
)->Result<()> {
    let first_completion=inputs.len();
    let (_,completions)=native_function::task_completion_parameters(parameters.iter().map(|parameter|match &parameter.kind{
        CallParameterKind::Value(ty)=>(ty.components.len(),Some(ty)),CallParameterKind::Reference{..}=>(2,None),
    }),0);
    let mut producer=completions.into_iter();let mut completion=None;let mut cursor=InputCursor::default();let mut facts=CompletionFacts::default();
    loop {
        let step=unsafe{completion_step(cursor,first_completion as u64,facts)};cursor=step.cursor;
        match step.need {
            CompletionNeed::Next=>{
                completion=producer.next();facts.present=u32::from(completion.is_some());
                if let Some(value)=&completion {
                    match value.parent(){Some((parent,component))=>{facts.nested=1;facts.parent=parent as u64;facts.component=component as u64;},None=>{facts.nested=0;facts.input=value.input() as u64;}}
                }
            },
            CompletionNeed::Publish=>{
                let completion=completion.take().unwrap();let source=match step.source{CompletionSource::Input=>TaskCompletionSource::Input(inputs[step.input as usize]),CompletionSource::Returned=>TaskCompletionSource::Returned(inputs[step.input as usize],step.component as usize)};
                let address=[b.reg(Scalar::U32),b.reg(Scalar::U32)];let layout=b.task_completion_layout(&completion.result,span)?;
                b.task_completions.entry(source).or_default().push((completion.result,address));parameters.push(CallParameter::reference(layout,Access::InOut));inputs.extend(address);
            },
            CompletionNeed::Finish=>return Ok(()),
        }
    }
}
pub(super) fn return_inputs(b:&mut Builder<'_>,c_abi:bool,result:Option<&Ty>,parameters:&mut Vec<CallParameter>,inputs:&mut Vec<Reg>) {
    let mut cursor=InputCursor::default();let mut facts=ReturnFacts{application:u32::from(b.application_entry()),c_abi:u32::from(c_abi),result:u32::from(result.is_some()),nonempty:0};let mut task_results=None;
    loop {
        let step=unsafe{return_step(cursor,facts)};cursor=step.cursor;
        match step.need {
            ReturnNeed::Inspect=>{let values=result.unwrap().task_components();facts.nonempty=u32::from(!values.is_empty());task_results=Some(values);},
            ReturnNeed::Publish=>{let address=[b.reg(Scalar::U32),b.reg(Scalar::U32)];b.task_return_source=Some(address);parameters.push(CallParameter::reference(Ty::task_return_layout(&task_results.take().unwrap()),Access::Write));inputs.extend(address);return;},
            ReturnNeed::Finish=>return,
        }
    }
}

struct ResourceInputs<'a> {
    definition:&'a Definition,env:&'a BTreeMap<String,Binding>,pending:Option<&'a application::Data>,
    resources:BTreeMap<ValueIdentity,bounds::Proof>,
}
unsafe extern "C" fn inspect_resource(context:*mut std::ffi::c_void,index:u64)->u32 {
    let context=unsafe{&mut *context.cast::<ResourceInputs<'_>>()};
    let env=context.env;let parameter=context.definition.function.params().at(index as usize);
    context.pending=match env.get(parameter.name()){Some(Binding::ApplicationData(data))=>Some(data),_=>None};
    u32::from(context.pending.is_some())
}
unsafe extern "C" fn publish_resource(context:*mut std::ffi::c_void,index:u64) {
    let context=unsafe{&mut *context.cast::<ResourceInputs<'_>>()};let mut proof=bounds::Proof::default();
    proof.set_view_borrow(0,view::resource_borrow(context.pending.take().unwrap()));
    context.resources.insert(ValueIdentity::ResourceParameter(index as usize,0),proof);
}
#[inline(never)]
pub(super) async fn function_parameters(
    b:&mut Builder<'_>,instance:&crate::generics::Instance,name:&str,bound_parameters:Vec<Param>,
)->Result<(Vec<CallParameter>,Vec<Reg>,BTreeSet<(ContractKind,ContractPredicate)>)> {
    let world=b.world;let d=&instance.definition;let mut inputs=Vec::new();let mut parameters=Vec::new();
    let mode=Mode{application:u32::from(b.application.is_some()),c_abi:u32::from(d.function.c_abi()),cooperative:u32::from(d.function.kind()==FunctionKind::Cooperative)};
    let context:(&Source,&str,&[String],FunctionKind)=(&d.source,&d.module,&d.imports,d.function.kind());
    let mut cursor=BindingCursor::default();let mut control=BindingFacts::default();let mut producer=bound_parameters.into_iter();
    let mut parameter=None;let mut binding=None;let mut ty=None;let mut paths=None;let mut resources=BTreeMap::new();let mut requirements=None;let mut output_contracts=BTreeSet::new();
    loop {
        let step=unsafe{binding_step(cursor,mode,control)};cursor=step.cursor;
        match step.need {
            BindingNeed::Frame=>{let frame=b.application_frame(d.function.span())?;parameters.push(CallParameter::value(frame.ty).with_origin(Some(Arc::new(ParameterOrigin::Frame))));inputs.extend(frame.regs);},
            BindingNeed::Calls=>b.calls.push(name.into()),
            BindingNeed::Cooperative=>{b.cooperative_root=Some(0);},
            BindingNeed::Next=>{parameter=producer.next();control.present=u32::from(parameter.is_some());if let Some(p)=&parameter{control.shape=facts(p);}},
            BindingNeed::Acceleration=>{binding=Some(Binding::Acceleration(cursor.acceleration as usize));},
            BindingNeed::Texture=>{
                let p=parameter.as_ref().unwrap();let address=b.reg(Scalar::U64);let origin=origin(b,d,name,cursor.index as usize,p);
                parameters.push(CallParameter::value(Ty::scalar(Scalar::U64)).with_origin(origin));inputs.push(address);
                let mut resource=texture::TextureReference::binding(cursor.texture as usize);resource.source.parameter=Some(address);binding=Some(Binding::Texture(resource));
            },
            BindingNeed::Dataset=>{binding=Some(unsafe{world.call(dataset_parameter(b,d,name,cursor.index as usize,cursor.data as usize,parameter.as_ref().unwrap(),&mut parameters,&mut inputs)).await}?);},
            BindingNeed::Type=>{ty=Some(unsafe{world.call(cooperative::parameter_type(world,parameter.as_ref().unwrap(),context,b.program.target)).await}?);},
            BindingNeed::Tile=>{
                let p=parameter.take().unwrap();let tile=b.tiles.len();b.tiles.push(CheckedTile{source:TileOrigin::Parameter(ty.take().unwrap()),width:TileWidth::Parameter,access:p.access});b.bind_tile(&p.name,tile);
            },
            BindingNeed::Ordinary=>{unsafe{world.call(ordinary_parameter(b,d,name,mode,cursor.index as usize,parameter.take().unwrap(),ty.take().unwrap(),&mut parameters,&mut inputs)).await}?;},
            BindingNeed::Bind=>{b.env.insert(parameter.take().unwrap().name,binding.take().unwrap());},
            BindingNeed::Elements=>{let mut env=std::mem::take(&mut b.env);paths=Some(unsafe{world.call(contracts::element_input_paths(b,instance.into(),&mut env,None)).await}?);b.env=env;},
            BindingNeed::BindElements=>application::bind_parameter_elements(b,instance.into(),&paths.take().unwrap()),
            BindingNeed::Resources=>{
                let mut context=ResourceInputs{definition:d,env:&b.env,pending:None,resources:BTreeMap::new()};
                unsafe{resource_parameters(d.function.params().len() as u64,(&mut context as *mut ResourceInputs<'_>).cast(),inspect_resource,publish_resource)};
                resources=context.resources;
            },
            BindingNeed::Resolve=>{
                let env=std::mem::take(&mut b.env);let resolved=unsafe{world.call(contracts::resolve(b,instance.into(),&LexicalScope::Function(instance.clone()),&env,&resources)).await};b.env=env;requirements=Some(resolved?);
            },
            BindingNeed::Assume=>{let mut env=std::mem::take(&mut b.env);unsafe{world.call(contracts::assume(b,instance.into(),&mut env,&requirements.take().unwrap())).await}?;b.env=env;},
            BindingNeed::Arrays=>b.materialize_array_bindings(),
            BindingNeed::Outputs=>{output_contracts=unsafe{world.call(outputs::contract_predicates(b,instance)).await}?;},
            BindingNeed::Completions=>completion_inputs(b,d.function.span(),&mut parameters,&mut inputs)?,
            BindingNeed::Finish=>return Ok((parameters,inputs,output_contracts)),
        }
    }
}
