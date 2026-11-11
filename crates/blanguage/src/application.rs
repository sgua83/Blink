//! Windows x64 应用编译为 COFF 目标文件；PE 链接由独立 linker 模块负责。
pub(crate) use crate::resource::RUNTIME_LIBRARY;
use crate::{
    ast::Target,
    ir::{Program, Ty, ValueCode},
    native,
    object::{self, Kind, Object},
};
use std::path::Path;
pub struct Application {
    pub(crate) compiler: crate::Compiler,
    pub(crate) name: String,
    pub(crate) body: ValueCode,
    pub(crate) functions: Vec<crate::ir::FunctionBody>,
    pub(crate) imports: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) resources: Option<Resources>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Resources {
    pub types: Vec<Ty>,
    pub programs: Vec<(Target, Program)>,
    pub completions: Vec<Vec<ElementCompletion>>,
    pub task_starts: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementCompletion {
    pub offset: u32,
    pub intervals: Vec<(u64, u64)>,
    pub known_bits: (u64, u64),
    pub limits: Vec<(usize, u32)>,
}
impl Application {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn compile(&self) -> Result<Object, Box<dyn std::error::Error>> {
        self.compiler.execute(|worker| self.compile_in(worker).map_err(|error| error.to_string()))
            .map_err(|error| std::io::Error::other(error.to_string()).into())
    }
    pub(crate) fn compile_in(&self, worker: &crate::compiler::Worker) -> Result<Object, Box<dyn std::error::Error>> {
        use std::borrow::Cow;
        let mut resources = self.resources.as_ref().map(Cow::Borrowed);
        let mut body = Cow::Borrowed(&self.body);
        let mut ordinary = Cow::Borrowed(self.functions.as_slice());
        let has_tasks = self.resources.as_ref().is_some_and(|resources| resources.task_starts);
        if has_tasks {
            bind_tasks(&self.name, Some(body.to_mut()), &self.functions, ordinary.to_mut(),
                &self.imports, resources.as_mut().map(Cow::to_mut)).map_err(std::io::Error::other)?;
        }
        let code = native::emit_application(&body).map_err(std::io::Error::other)?;
        let mut stack_bytes = code.stack_bytes;
        let mut functions = crate::parallel::try_map(worker, ordinary.iter().collect(), |_| (),
            |_, function| native::emit_function(function).map(|code| (function.name.clone(), code)))
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let (archive, resource_code, resource_imports) = compile_resources(&self.name, resources.as_deref(), true, worker, has_tasks)?;
        let resource_names: Vec<_> = resource_code.iter().map(|(name, _)| name.clone()).collect();
        functions.extend(resource_code);
        let mut imports = self.imports.clone();
        imports.extend(resource_imports);
        for (_, function) in &functions {
            stack_bytes = stack_bytes.max(function.stack_bytes);
        }
        let (sections, positions, stack) =
            object::application_code(code, functions, &imports, &resource_prefix(&self.name), &self.name)?;
        let symbols = std::iter::once("main".to_owned()).chain(resource_names).map(|name| object::Symbol {
            section: positions[&name].0, value: positions[&name].1,
            name, contract: Vec::new(), function: true,
        }).collect();
        Ok(Object {
            name: self.name.clone(),
            kind: Kind::Application { stack_bytes },
            sections,
            stack,
            symbols,
            imports,
            archive,
        })
    }
    pub fn write_object(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.compile()?.write(path)
    }
}

pub(crate) fn resource_prefix(name: &str) -> String {
    use std::fmt::Write;
    let mut prefix = String::from("blink_m");
    for byte in name.bytes() {
        write!(prefix, "{byte:02x}").unwrap();
    }
    prefix
}
pub(crate) fn bind_tasks(
    module:&str,body:Option<&mut ValueCode>,definitions:&[crate::ir::FunctionBody],
    functions:&mut Vec<crate::ir::FunctionBody>,imports:&std::collections::BTreeMap<String,Vec<u8>>,
    resources:Option<&mut Resources>,
) -> Result<bool,&'static str> {
    let Some(resources)=resources else { return Ok(false); };
    let start_symbol=format!("{}_task_start",resource_prefix(module));
    let mut tasks=Vec::new();
    for caller in body.into_iter().chain(functions.iter_mut().map(|function| &mut function.body)) {
        bind_task_entries(caller,definitions,imports,resources,&mut tasks,&start_symbol)?;
    }
    let has_tasks=!tasks.is_empty();
    for binding in tasks {
        functions.push(binding.entry);
        functions.extend(binding.cleanup);
    }
    Ok(has_tasks)
}

struct TaskBinding {
    worker: String,
    input: Ty,
    start: Ty,
    index: usize,
    entry: crate::ir::FunctionBody,
    cleanup: Option<crate::ir::FunctionBody>,
}

fn bind_task_entries(
    body:&mut ValueCode,functions:&[crate::ir::FunctionBody],imports:&std::collections::BTreeMap<String,Vec<u8>>,
    resources:&mut Resources,tasks:&mut Vec<TaskBinding>,start_symbol:&str,
) -> Result<(), &'static str> {
    use crate::ir::{CallArgument,CallTarget,Inst,InstKind,Scalar};
    use std::collections::BTreeMap;
    let mut addresses = BTreeMap::new();
    crate::optimize::visit(&body.code,&mut |inst| {
        if let InstKind::FunctionAddress(register,CallTarget::Defined(name))=&inst.kind {
            addresses.insert(*register,name.clone());
        }
    });
    fn block(
        code:&mut Vec<Inst>,registers:&mut Vec<Scalar>,addresses:&BTreeMap<usize,String>,
        functions:&[crate::ir::FunctionBody],imports:&BTreeMap<String,Vec<u8>>,resources:&mut Resources,tasks:&mut Vec<TaskBinding>,start_symbol:&str,
    ) -> Result<bool,&'static str> {
        let mut changed = false;
        let mut output = Vec::with_capacity(code.len());
        for mut inst in std::mem::take(code) {
            match &mut inst.kind {
                InstKind::Call(call)
                    if matches!(&call.target,CallTarget::External(name) if name.symbol=="blink_task_start") =>
                {
                    let CallArgument::Reference{ty:input,..}=&call.arguments[1] else {
                        return Err("task start needs an actual input packet");
                    };
                    let CallArgument::Value{values,..}=&call.arguments[2] else {
                        return Err("task start needs a bound worker");
                    };
                    let worker=addresses.get(&values[0]).ok_or("missing actual task function address")?;
                    let CallArgument::Value{values,..}=&call.arguments[3] else { return Err("missing task cleanup argument"); };
                    let cleanup=addresses.get(&values[0]);
                    let start=call.result.as_ref().ok_or("missing task start result type")?;
                    let binding=if let Some(index)=tasks.iter().position(|task| {
                        task.worker==*worker && task.input==*input && task.start==*start
                    }) {
                        &tasks[index]
                    } else {
                        let (parameters,result,fallible)=if let Some(function)=functions.iter().find(|function|function.name==*worker) {
                            (function.parameters.clone(),function.result.clone(),function.fallible)
                        } else {
                            crate::check::compiled_task_signature(imports.get(worker).ok_or("missing compiled ordinary worker contract")?)?
                        };
                        let mut entry=task_entry(worker,&parameters,result.as_ref(),fallible,input)?;
                        entry.name=format!("{worker}@task{}",tasks.len());
                        let result=result.unwrap_or_else(Ty::unit);
                        let cleanup=cleanup.map(|name| {
                            let function=functions.iter().find(|function|function.name==*name)
                                .ok_or("missing owned task result cleanup")?;
                            let mut packet=Ty::unit();
                            packet.fields.push(("value".into(),0,Box::new(result.clone())));
                            let mut cleanup=task_entry(name,&function.parameters,None,function.fallible,&packet)?;
                            cleanup.name=format!("{worker}@task_cleanup{}",tasks.len());
                            Ok::<_,&'static str>(cleanup)
                        }).transpose()?;
                        let index=resources.types.len();
                        resources.types.extend([input.clone(),result.clone(),start.clone()]);
                        tasks.push(TaskBinding{worker:worker.clone(),input:input.clone(),start:start.clone(),index,entry,cleanup});
                        tasks.last().unwrap()
                    };
                    let entry=registers.len();
                    registers.extend([Scalar::U64,Scalar::U32]);
                    output.push(Inst::new(InstKind::FunctionAddress(entry,CallTarget::Defined(binding.entry.name.clone()))));
                    output.push(Inst::new(InstKind::Constant(entry+1,Scalar::U32,binding.index as u64)));
                    let value=|scalar,reg|CallArgument::Value{ty:Ty::scalar(scalar),values:vec![reg],dataset_borrow:None,origin:None};
                    call.arguments[2]=value(Scalar::U64,entry);
                    if let Some(cleanup)=&binding.cleanup {
                        let register=registers.len();
                        registers.push(Scalar::U64);
                        output.push(Inst::new(InstKind::FunctionAddress(register,CallTarget::Defined(cleanup.name.clone()))));
                        call.arguments[3]=value(Scalar::U64,register);
                    }
                    call.arguments.push(value(Scalar::U32,entry+1));
                    let CallTarget::External(target)=&mut call.target else { unreachable!() };
                    target.symbol=start_symbol.into();
                    changed=true;
                }
                InstKind::Scope(body) => changed |= block(body,registers,addresses,functions,imports,resources,tasks,start_symbol)?,
                InstKind::If{yes,no,..} => {
                    changed |= block(yes,registers,addresses,functions,imports,resources,tasks,start_symbol)?;
                    changed |= block(no,registers,addresses,functions,imports,resources,tasks,start_symbol)?;
                }
                InstKind::Loop{condition,body,..} => {
                    changed |= block(condition,registers,addresses,functions,imports,resources,tasks,start_symbol)?;
                    changed |= block(body,registers,addresses,functions,imports,resources,tasks,start_symbol)?;
                }
                _ => {}
            }
            output.push(inst);
        }
        *code=output;
        Ok(changed)
    }
    if block(&mut body.code,&mut body.registers,&addresses,functions,imports,resources,tasks,start_symbol)? {
        crate::optimize::value_code(body);
    }
    Ok(())
}

/// Reuse the ordinary x64 call emitter for the actual compiled function signature.
fn task_entry(worker:&str,parameters:&[crate::ir::CallParameter],result:Option<&Ty>,fallible:bool,packet:&Ty) -> Result<crate::ir::FunctionBody,&'static str> {
    use crate::ir::{BinaryOp,Call,CallArgument,CallParameter,CallParameterKind,CallTarget,Comparison,FunctionBody,IndexedSource,Inst,InstKind,Scalar};
    let pointer=Ty::vector(Scalar::U32,2);
    let mut body=ValueCode{entry_origin:None,generated_origin:None,registers:vec![Scalar::U32;6],
        locals:Default::default(),code:Vec::new(),inputs:(0..6).collect(),outputs:Vec::new()};
    let zero=body.registers.len();
    body.registers.push(Scalar::U32);
    body.code.push(Inst::new(InstKind::Constant(zero,Scalar::U32,0)));
    let buffer=|address|IndexedSource::Pointer{address,index:zero,stride:1,aliases:Vec::new()};
    let mut arguments=Vec::new();
    let mut fields=packet.fields.iter();
    for parameter in parameters {
        let CallParameterKind::Value(ty)=&parameter.kind else { return Err("task requires by-value parameters"); };
        let values=if ty.name=="__blink_frame" { vec![0,1] } else {
            let Some((_,base,field))=fields.next() else { return Err("missing task argument"); };
            if field.as_ref()!=ty { return Err("task input layout differs from the ordinary parameter"); }
            let mut registers=Vec::new();
            for &(scalar,offset) in &ty.components {
                let dst=body.registers.len();
                body.registers.push(scalar);
                body.code.push(Inst::new(InstKind::LoadIndexed{dst,source:buffer([2,3]),offset:zero,word:*base+offset}));
                registers.push(dst);
            }
            registers
        };
        if ty.size!=0 {
            arguments.push(CallArgument::Value{ty:ty.clone(),values,dataset_borrow:None,origin:None});
        }
    }
    if fields.next().is_some() { return Err("unused task argument"); }
    let values:Vec<_>=result.into_iter().flat_map(|ty|&ty.components).map(|(scalar,_)|{
        let dst=body.registers.len();body.registers.push(*scalar);dst
    }).collect();
    let error=fallible.then(||{let dst=body.registers.len();body.registers.push(Scalar::U64);dst});
    body.code.push(Inst::new(InstKind::Call(Call{
        failure:None,error,target:CallTarget::Defined(worker.into()),arguments,
        memory:Vec::new(),memory_use:None,result:result.cloned(),values:values.clone(),
    })));
    if let Some(error)=error {
        let none=body.registers.len();
        body.registers.push(Scalar::U64);
        body.code.push(Inst::new(InstKind::Constant(none,Scalar::U64,0)));
        let failed=body.registers.len();
        body.registers.push(Scalar::Bool);
        body.code.push(Inst::new(InstKind::Binary{dst:failed,op:BinaryOp::Compare(Comparison::NotEqual),a:error,b:none}));
        body.code.push(Inst::new(InstKind::If{condition:failed,yes:vec![Inst::new(InstKind::Raise(error))],no:Vec::new()}));
    }
    if let Some(result)=result {
        for ((_,offset),value) in result.components.iter().zip(values) {
            body.code.push(Inst::new(InstKind::StoreIndexed{source:buffer([4,5]),offset:zero,word:*offset,value}));
        }
    }
    Ok(FunctionBody{
        name:format!("{worker}@task"),fallible:true,source_origin:None,
        parameters:vec![CallParameter::value(pointer);3],result:None,body,
    })
}

pub(crate) fn compile_resources(
    name: &str,
    resources: Option<&Resources>,
    root: bool,
    worker: &crate::compiler::Worker,
    has_tasks: bool,
) -> Result<(Option<object::Archive>, Vec<(String, native::NativeCode)>, std::collections::BTreeMap<String, Vec<u8>>), Box<dyn std::error::Error>> {
    Ok(if let Some(resources) = resources {
        let prefix = resource_prefix(name);
        let (image, code, imports) = crate::resource::compile(resources, worker)?;
        let code: Vec<_> = code.into_iter().enumerate()
            .map(|(index, code)| (format!("__blink_range_{prefix}_{index}"), code)).collect();
        let mut source = crate::dll::application_source(resources, &prefix, root, &image, &code)?;
        if has_tasks {
            source.push_str(&include_str!("task.rs.in")
                .replace("TYPE_BASE", &format!("{prefix}_type_base"))
                .replace("START_SYMBOL", &format!("{prefix}_task_start")));
        }
        let mut exports = if root {
            [
                "blink_destroy",
                "blink_release",
                "blink_upload",
                "blink_download",
                "blink_address",
                "blink_storage_address",
                "blink_allocate",
                "blink_resize",
                "blink_run",
                "blink_complete_field",
                "blink_error_message",
                "blink_error_destroy",
                "blink_error_join",
                "blink_task_start",
                "blink_task_wait",
                "blink_application_failed",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        exports.extend((0..resources.types.len()).map(|i| format!("{prefix}_allocate_{i}")));
        exports.extend((0..resources.programs.len()).map(|i| format!("{prefix}_run_{i}")));
        exports.extend((0..resources.completions.len()).map(|i| format!("{prefix}_complete_{i}")));
        if root {
            exports.extend(["blink_create".into(), "blink_application_error".into()]);
        }
        if has_tasks { exports.push(format!("{prefix}_task_start")); }
        exports.push(format!("{prefix}_resources"));
        let dependencies: &[&str] = if root {
            &["blanguage", "blcompute"]
        } else {
            &[]
        };
        let mut archive = object::compile_archive(&source, exports, dependencies, worker)?;
        archive.resources = Some(object::ResourceBinding {
            prefix,
            types: u32::try_from(resources.types.len())?,
            programs: u32::try_from(resources.programs.len())?,
            abi: crate::resource::ABI.to_vec(),
            code: code.iter().map(|(name, _)| name.clone()).collect(),
        });
        (Some(archive), code, imports)
    } else {
        (None, Vec::new(), std::collections::BTreeMap::new())
    })
}
