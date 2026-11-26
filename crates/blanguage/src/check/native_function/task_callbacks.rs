//! B generates callback inputs and selection writes through the native Builder.
use super::*;
use crate::check::application::TaskOutputSelection;
use std::{ffi::c_void, ptr};

#[repr(C)]
struct InputHost {
    frame: unsafe extern "C" fn(*mut c_void) -> u32,
    parameter: unsafe extern "C" fn(*mut c_void, usize),
}

#[repr(C)]
struct SelectionCounts {
    packet_components: usize,
    packet_registers: usize,
    result_components: usize,
}

#[repr(C)]
struct SelectionHost {
    packet: unsafe extern "C" fn(*mut c_void, usize),
    result: unsafe extern "C" fn(*mut c_void, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.task_callbacks::callback_inputs"]
    fn callback_inputs(frame: u32, count: usize, context: *mut c_void, host: InputHost) -> u32;
    #[link_name = "bootstrap.task_callbacks::selection_inputs"]
    fn selection_inputs(counts: SelectionCounts, context: *mut c_void, host: SelectionHost);
    #[link_name = "bootstrap.task_callbacks::selection_ready"]
    fn selection_ready(
        owners: usize,
        base: u32,
        stride: u32,
        context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, usize, u32),
    );
}

struct CallbackInputs<'a, 'b> {
    builder: &'a mut Builder<'b>,
    arguments: &'a [(&'a str, Ty)],
    span: Span,
    inputs: Vec<Reg>,
    abi: Vec<CallParameter>,
    frame_inputs: usize,
    error: Option<crate::ast::Diagnostic>,
}

unsafe extern "C" fn frame_input(context: *mut c_void) -> u32 {
    let context = unsafe { &mut *context.cast::<CallbackInputs<'_, '_>>() };
    context.builder.application = Some(application::Context::default());
    match context.builder.application_frame(context.span) {
        Ok(frame) => {
            context.inputs.extend(frame.regs);
            context.frame_inputs = context.inputs.len();
            context.abi.push(CallParameter::value(frame.ty)
                .with_origin(Some(Arc::new(ParameterOrigin::Frame))));
            0
        }
        Err(error) => {
            context.error = Some(error);
            1
        }
    }
}

unsafe extern "C" fn parameter_input(context: *mut c_void, index: usize) {
    let context = unsafe { &mut *context.cast::<CallbackInputs<'_, '_>>() };
    let ty = &context.arguments[index].1;
    context.inputs.extend(ty.components.iter().map(|&(scalar, _)| context.builder.reg(scalar)));
    context.abi.push(CallParameter::value(ty.clone()));
}

struct SelectionInputs<'a, 'b> {
    builder: &'a mut Builder<'b>,
    packet: &'a Value,
    result: &'a Ty,
    input: [Reg; 2],
    output: [Reg; 2],
    zero: Reg,
    snapshot: BTreeMap<Reg, Reg>,
    returned_regs: Vec<Reg>,
}

impl SelectionInputs<'_, '_> {
    fn load(&mut self, address: [Reg; 2], (scalar, word): (Scalar, u32)) -> Reg {
        let dst = self.builder.reg(scalar);
        self.builder.emit(InstKind::LoadIndexed {
            dst,
            source: IndexedSource::Pointer { address, index: self.zero, stride: 1, aliases: Vec::new() },
            offset: self.zero,
            word,
        });
        dst
    }
}

unsafe extern "C" fn snapshot_input(context: *mut c_void, index: usize) {
    let context = unsafe { &mut *context.cast::<SelectionInputs<'_, '_>>() };
    let loaded = context.load(context.input, context.packet.ty.components[index]);
    context.snapshot.insert(context.packet.regs[index], loaded);
}

unsafe extern "C" fn snapshot_result(context: *mut c_void, index: usize) {
    let context = unsafe { &mut *context.cast::<SelectionInputs<'_, '_>>() };
    let loaded = context.load(context.output, context.result.components[index]);
    context.returned_regs.push(loaded);
}

struct ReadyOutput<'a, 'b, 'c> {
    builder: &'a mut Builder<'b>,
    item: &'a TaskOutputSelection<'c>,
    selected: &'a [owned_presence::Presence],
    output: [Reg; 2],
    zero: Reg,
}

unsafe extern "C" fn ready_output(context: *mut c_void, owner: usize, word: u32) {
    let context = unsafe { &mut *context.cast::<ReadyOutput<'_, '_, '_>>() };
    let offset = context.item.owners[owner].0;
    let ready = outputs::resource_guard(&context.item.guard.ty, offset, context.selected);
    let value = context.builder.presence_register(&ready, None);
    context.builder.emit(InstKind::StoreIndexed {
        source: IndexedSource::Pointer {
            address: context.output,
            index: context.zero,
            stride: 1,
            aliases: Vec::new(),
        },
        offset: context.zero,
        word,
        value,
    });
}

impl Builder<'_> {
    pub(in crate::check::native_function) async fn task_callback(
        &mut self,
        name: String,
        arguments: &[(&str, Ty)],
        frame: bool,
        span: Span,
        emit: impl std::ops::AsyncFnOnce(&mut Builder<'_>, &[Reg]) -> Result<()>,
    ) -> Result<String> {
        let parameters: Arc<[Param]> = arguments.iter().map(|(name, ty)| Param {
            name: (*name).into(),
            ty: ty.syntax(),
            access: Access::Value,
            span,
        }).collect::<Vec<_>>().into();
        let lexical = LexicalScope::Values {
            source: self.scope.source().clone(),
            module: self.scope.module().into(),
            imports: self.scope.imports().to_vec(),
            bindings: self.scope.bindings().clone(),
            span,
            parameters: parameters.clone(),
            source_definition: None,
            source_context: Arc::new(SourceContext {
                instance: Some(Arc::from(name.as_str())),
                inlined_at: Box::default(),
                domain_path: Arc::from([]),
            }),
        };
        let program = Arc::new(unsafe {
            self.world.call(function_program(
                self.world,
                &name,
                parameters.iter().map(|parameter| std::future::ready(Ok(parameter))),
                (lexical.source(), lexical.module(), lexical.imports(), FunctionKind::Value),
                Target::Cpu,
                None,
                |_| {},
            )).await
        }?);
        let (definition, context) = lexical.syntax_context(Some(&name));
        let mut builder = Box::new(Builder::from_checked(
            self.world, &lexical, &program, 0, definition, context, BindingUses::default(),
        ));
        builder.in_function = true;
        let mut context = CallbackInputs {
            builder: &mut builder,
            arguments,
            span,
            inputs: Vec::new(),
            abi: Vec::new(),
            frame_inputs: 0,
            error: None,
        };
        let host = InputHost { frame: frame_input, parameter: parameter_input };
        unsafe {
            callback_inputs(u32::from(frame), arguments.len(), ptr::from_mut(&mut context).cast(), host);
        }
        let CallbackInputs { inputs, abi, frame_inputs, error, .. } = context;
        if let Some(error) = error {
            return Err(error);
        }
        builder.functions = std::mem::take(&mut self.functions);
        emit(&mut builder, &inputs[frame_inputs..]).await?;
        let checked = unsafe {
            self.world.call(finish_checked_function(
                builder, program.clone(), &name, span, None, None, abi, inputs, BTreeSet::new(),
                None, vec![None; parameters.len()],
            )).await
        }?;
        self.functions = finish_declaration(self.world, &lexical, false, checked)?;
        if frame {
            let contract = &mut self.functions.units.get_mut(&name).unwrap().as_mut().unwrap().contract;
            contract.frame = true;
            contract.fallible = true;
        }
        Ok(name)
    }

    pub(in crate::check) async fn task_selection(
        &mut self,
        packet: &Value,
        result: &Ty,
        outputs: &[TaskOutputSelection<'_>],
        span: Span,
    ) -> Result<String> {
        let name = format!("{}::@task_selection{}", self.scope.module(), self.functions.units.len());
        let pointer = Ty::vector(Scalar::U32, 2);
        unsafe {
            self.world.call(self.task_callback(
                name, &[("input", pointer.clone()), ("output", pointer)], false, span,
                async |builder, registers| {
                    let [a, b, c, d] = *registers else { unreachable!() };
                    let (input, output) = ([a, b], [c, d]);
                    let zero = builder.constant(Scalar::U32, 0).regs[0];
                    let counts = SelectionCounts {
                        packet_components: packet.ty.components.len(),
                        packet_registers: packet.regs.len(),
                        result_components: result.components.len(),
                    };
                    let mut context = SelectionInputs {
                        builder, packet, result, input, output, zero,
                        snapshot: BTreeMap::new(),
                        returned_regs: Vec::new(),
                    };
                    let host = SelectionHost { packet: snapshot_input, result: snapshot_result };
                    selection_inputs(counts, ptr::from_mut(&mut context).cast(), host);
                    let SelectionInputs { snapshot, returned_regs, .. } = context;
                    let returned = Value {
                        ty: result.clone(),
                        regs: returned_regs,
                        proof: bounds::Proof::default(),
                        uniform: false,
                        literal: false,
                        constant: None,
                    };
                    for item in outputs {
                        let selected = builder.world.call(item.guard.guards(
                            builder, Some(&returned), span, |operand| match operand {
                                bounds::Operand::Value(ValueIdentity::Register(reg)) => snapshot.get(&reg)
                                    .map(|reg| bounds::Operand::Value(ValueIdentity::Register(*reg))),
                                _ => Some(operand),
                            },
                        )).await?;
                        let mut context = ReadyOutput { builder, item, selected: &selected, output, zero };
                        selection_ready(
                            item.owners.len(), item.base, Scalar::Bool.size(),
                            ptr::from_mut(&mut context).cast(), ready_output,
                        );
                    }
                    Ok(())
                },
            )).await
        }
    }
}
