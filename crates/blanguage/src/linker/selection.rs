//! B owns this link's definition tree, section queue, resource closure and remapping.
//! Rust keeps the original objects/libraries and only hands their current borrowed views across the synchronous call.
use super::{EntryPlan, EntryProduct, Inputs, LibraryBinding, Result, Roots, Selected, Selection, Source, resources};
use crate::{names::ModuleName, object::{self, Object, Kind, Target, Library, ReadSection}};
use resources::ImportBytes as Bytes;
use std::{ffi::c_void, marker::PhantomData, mem::MaybeUninit};
#[derive(Clone,Copy)]#[repr(C)]
struct ObjectView {
    kind:u64,name:Bytes,section_start:u64,section_count:u64,symbol_start:u64,symbol_count:u64,
    import_start:u64,import_count:u64,export_start:u64,export_count:u64,code_start:u64,code_count:u64,
    frame_start:u64,frame_count:u64,prefix:Bytes,archive:u64,has_resources:u64,selected:u64,active_archive:u64,keep_archive:u64,borrowed:u64,stack_bytes:u64,
}
#[derive(Clone,Copy)]#[repr(C)]
struct SectionView {owner:u64,flags:u64,associated:*const usize,associated_count:u64,relocation_start:u64,relocation_count:u64,live:u64,mapping:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct RelocationView {kind:u64,section:u64,name:Bytes,dll:Bytes,rewrite:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct SymbolView {name:Bytes,contract:Bytes,owner:u64,section:u64,keep:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct ImportView {name:Bytes,contract:Bytes}
#[derive(Clone,Copy)]#[repr(C)]
struct ExportView {name:Bytes,keep:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct FrameView {section:u64,present:u64,keep:u64,mapping:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct LocalView {slot:Bytes,name:Bytes}
#[repr(C)]
struct RuntimeView {name:Bytes,used:u64}
#[derive(Clone,Copy)]#[repr(C)]
pub(crate) struct RootView<'a> {
    name: ModuleName<'a>,
    contract: Bytes,
    owner: PhantomData<&'a [u8]>,
}
unsafe extern "C" {
    #[link_name = "bootstrap.native_select::bind_root_contracts"]
    fn bind_native_roots(roots: *mut RootView<'_>, count: usize, context: *const c_void, sources: usize,
        lookup: unsafe extern "C" fn(*const c_void, usize, Bytes) -> Bytes) -> usize;
}

unsafe extern "C" fn lookup_contract<'a, F>(context: *const c_void, index: usize, name: Bytes) -> Bytes
where
    F: Fn(usize, &str) -> Option<&'a [u8]>,
{
    let lookup = unsafe { &*context.cast::<F>() };
    lookup(index, text(&name)).map_or(Bytes { bytes: std::ptr::null(), length: 0 }, borrowed)
}

pub(crate) fn bind_root_contracts<'a, F>(
    names: Vec<ModuleName<'a>>, source_count: usize, lookup: F,
) -> std::result::Result<Vec<RootView<'a>>, String>
where
    F: Fn(usize, &str) -> Option<&'a [u8]>,
{
    let mut roots: Vec<_> = names.into_iter().map(|name| RootView {
        name, contract: borrowed(&[]), owner: PhantomData,
    }).collect();
    let missing = unsafe {
        bind_native_roots(roots.as_mut_ptr(), roots.len(), std::ptr::from_ref(&lookup).cast(),
            source_count, lookup_contract::<F>)
    };
    if missing == 0 {
        Ok(roots)
    } else {
        Err(format!("缺少模块函数接口 {}", roots[missing - 1].name.as_str()))
    }
}
#[repr(C)]
struct View {
    objects:*mut ObjectView,object_count:u64,sections:*mut SectionView,
    relocations:*mut RelocationView,symbols:*mut SymbolView,imports:*mut ImportView,
    exports:*mut ExportView,code:*mut Bytes,frames:*mut FrameView,nodes:*mut [u64;9],pending:*mut u64,
    locals:*mut LocalView,runtime:*mut RuntimeView,runtime_count:u64,library_count:u64,
}
#[derive(Clone,Copy)]#[repr(C)]
struct Params<'a> {native_mode:u64,roots:*const RootView<'a>,root_count:u64,types:u64,programs:u64,runtime_library:Bytes}
#[repr(u32)]
enum AppendStatus { Unchanged, Appended, Failed }
#[derive(Clone,Copy)]#[repr(C)]
struct ResourceRequest {runtime:*const RuntimeView,runtime_count:u64,locals:*const LocalView,local_count:u64,types:u64,programs:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct SelectionResult {error:u64,name:Bytes,owner:Bytes,entry:u64,plan:EntryPlan}
type Extract = unsafe extern "C" fn(*mut c_void, *mut View, u64, Bytes) -> AppendStatus;
type NativeArchive = unsafe extern "C" fn(*mut c_void, u64) -> u64;
type ResourceBinding = unsafe extern "C" fn(*mut c_void, *mut View, ResourceRequest) -> AppendStatus;
type Slot=unsafe extern "C" fn(*mut c_void,*mut View,Bytes)->Bytes;
unsafe extern "C" {
    #[link_name="bootstrap.native_select::choose"]
    fn choose(input:View,params:Params<'_>,context:*mut c_void,extract:Extract,native_archive:NativeArchive,resources:ResourceBinding,slot:Slot)->SelectionResult;
}
fn borrowed(bytes:&[u8])->Bytes {Bytes{bytes:bytes.as_ptr(),length:bytes.len() as u64}}
fn text(bytes:&Bytes)->&str {unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytes.bytes,bytes.length as usize))}}
struct NativeWork {
    locals: Vec<MaybeUninit<LocalView>>,
    runtime: Vec<RuntimeView>,
    slots: Vec<String>,
}
impl NativeWork {
    fn new() -> Self {
        let runtime = crate::resource::MODULE_RUNTIME_SYMBOLS.iter()
            .map(|name| RuntimeView { name: Bytes::new(name), used: 0 }).collect();
        Self { locals: Vec::new(), runtime, slots: Vec::new() }
    }
}
struct Context<'a, Payload> {
    sources:Vec<Source<'a, Payload>>,libraries:Vec<Library>,objects:Vec<ObjectView>,sections:Vec<SectionView>,
    relocations:Vec<RelocationView>,symbols:Vec<SymbolView>,imports:Vec<ImportView>,
    exports:Vec<ExportView>,code:Vec<Bytes>,frames:Option<Vec<FrameView>>,
    nodes:Vec<MaybeUninit<[u64;9]>>,pending:Vec<MaybeUninit<u64>>,
    native:Option<NativeWork>,error:Option<Box<dyn std::error::Error>>,
}
impl<'a, Payload> Context<'a, Payload> {
    fn new(inputs: Inputs<'a, Payload>, stack_output: bool, native: bool)->Self {
        let Inputs { sources, libraries } = inputs;
        let mut context=Self{sources:Vec::new(),libraries,objects:Vec::new(),sections:Vec::new(),relocations:Vec::new(),
            symbols:Vec::new(),imports:Vec::new(),exports:Vec::new(),code:Vec::new(),frames:stack_output.then(Vec::new),nodes:Vec::new(),pending:Vec::new(),
            native:native.then(NativeWork::new),error:None};
        for (index, source) in sources.iter().enumerate() {
            match source {
                Source::Owned(object) => context.describe(index, object, false),
                Source::Borrowed(object) => context.describe(index, object, true),
            }
        }
        context.sources = sources;
        context.grow();context
    }
    fn describe<SectionPayload>(&mut self, index: usize, object: &Object<SectionPayload>, borrowed_source: bool) {
        let empty=Bytes::new("");
        let (kind, stack_bytes) = match object.kind {
            Kind::Module { stack_bytes, .. } => (0, stack_bytes),
            Kind::Application { stack_bytes } => (1, stack_bytes),
            Kind::Library { .. } => (2, 0),
        };
        let mut row=ObjectView{
            kind,name:Bytes::new(&object.name),
            section_start:self.sections.len() as u64,section_count:object.sections.len() as u64,symbol_start:self.symbols.len() as u64,symbol_count:object.symbols.len() as u64,
            import_start:self.imports.len() as u64,import_count:object.imports.len() as u64,export_start:self.exports.len() as u64,export_count:0,
            code_start:self.code.len() as u64,code_count:0,frame_start:self.frames.as_ref().map_or(0, Vec::len) as u64,frame_count:0,prefix:empty,
            archive:object.archive.is_some() as u64,has_resources:0,selected:0,active_archive:0,keep_archive:0,borrowed:borrowed_source as u64,stack_bytes:u64::from(stack_bytes),
        };
        for section in &object.sections {
            // The original object owns its association lists through the synchronous selection.
            self.sections.push(SectionView{owner:index as u64,flags:section.flags.into(),associated:section.associated.as_ptr(),associated_count:section.associated.len() as u64,
                relocation_start:self.relocations.len() as u64,relocation_count:section.relocations.len() as u64,live:0,mapping:0});
            for relocation in &section.relocations{
                let(kind,section,name,dll)=match &relocation.target{
                    Target::Section(section,_)=>(0,*section as u64,empty,empty),
                    Target::Symbol(name)=>(1,0,Bytes::new(name),empty),
                    Target::Import(import)=>(2,0,Bytes::new(&import.name),Bytes::new(&import.dll)),
                };
                self.relocations.push(RelocationView{kind,section,name,dll,rewrite:0});
            }
        }
        self.symbols.extend(object.symbols.iter().map(|symbol| SymbolView {
            name: Bytes::new(&symbol.definition.name), contract: borrowed(&symbol.contract),
            owner: index as u64, section: symbol.definition.section as u64, keep: 0,
        }));
        self.imports.extend(object.imports.iter().map(|(name,contract)|ImportView{name:Bytes::new(name),contract:borrowed(contract)}));
        if let Some(archive)=&object.archive{
            row.export_count = archive.exports.len() as u64;
            let keep = u64::from(kind != 0);
            self.exports.extend(archive.exports.iter().map(|name| ExportView { name: Bytes::new(name), keep }));
            if let Some(resource)=&archive.resources{
                row.has_resources=1;row.prefix=Bytes::new(&resource.prefix);row.code_count=resource.code.len() as u64;
                self.code.extend(resource.code.iter().map(|name|Bytes::new(name)));
            }
        }
        if let Some(frames) = &mut self.frames {
            row.frame_count=object.stack.len() as u64;
            frames.extend(object.stack.iter().map(|frame|FrameView{section:frame.coff.map_or(0,|x|x.0 as u64),present:frame.coff.is_some() as u64,keep:0,mapping:0}));
        }
        self.objects.push(row);
    }
    fn grow(&mut self) {
        // Previously selected local slots share the tree with new object definitions.
        let local_slots = self.native.as_ref().map_or(0, |work| work.locals.len());
        let nodes = self.symbols.len() + local_slots;
        // B initializes each node or queue entry before making it reachable.
        self.nodes.resize_with(nodes, MaybeUninit::uninit);
        self.pending.resize_with(self.sections.len(), MaybeUninit::uninit);
    }
    fn view(&mut self)->View {
        let (locals, runtime, runtime_count) = self.native.as_mut().map_or(
            (std::ptr::null_mut(), std::ptr::null_mut(), 0),
            |work| (work.locals.as_mut_ptr().cast(), work.runtime.as_mut_ptr(), work.runtime.len() as u64),
        );
        View{objects:self.objects.as_mut_ptr(),object_count:self.objects.len() as u64,sections:self.sections.as_mut_ptr(),
            relocations:self.relocations.as_mut_ptr(),symbols:self.symbols.as_mut_ptr(),imports:self.imports.as_mut_ptr(),
            exports:self.exports.as_mut_ptr(),code:self.code.as_mut_ptr(),frames:self.frames.as_mut().map_or(std::ptr::null_mut(), Vec::as_mut_ptr),nodes:self.nodes.as_mut_ptr().cast(),pending:self.pending.as_mut_ptr().cast(),
            locals,runtime,runtime_count,library_count:self.libraries.len() as u64}
    }
    fn append_result(&mut self, view: &mut View, result: Result<Option<Object<Payload>>>) -> AppendStatus {
        match result {
            Ok(Some(object)) => {
                let index = self.sources.len();
                self.describe(index, &object, false);
                self.sources.push(Source::Owned(object));
                self.grow();
                // Appending can move the buffers borrowed by the B selector.
                *view = self.view();
                AppendStatus::Appended
            }
            Ok(None) => AppendStatus::Unchanged,
            Err(error) => {
                self.error = Some(error);
                AppendStatus::Failed
            }
        }
    }
    fn selected_relocations(&self, row: SectionView) -> &[RelocationView] {
        let start = row.relocation_start as usize;
        &self.relocations[start..start + row.relocation_count as usize]
    }

    fn slot_target(&self, view: &RelocationView) -> Option<Target> {
        if view.rewrite == 0 {
            return None;
        }
        let work = self.native.as_ref().unwrap();
        Some(Target::Symbol(work.slots[view.rewrite as usize - 1].clone()))
    }
    fn materialize(
        &self, source: Source<'a, Payload>, row: ObjectView,
        borrowed: &impl Fn(&'a Vec<u8>) -> Payload, write_coff: bool,
    ) -> Selected<Payload> {
        let sections=&self.sections[row.section_start as usize..(row.section_start+row.section_count) as usize];
        let symbols=&self.symbols[row.symbol_start as usize..(row.symbol_start+row.symbol_count) as usize];
        let mut object=match source {
            Source::Owned(object) => {
                let Object { name, kind, sections: input, symbols: declared, archive, mut stack, .. } = object;
                let sections = input.into_iter().zip(sections).filter_map(|(mut section, &row)| {
                    if row.live == 0 { return None; }
                    // MSVC consumes the same edges when writing the selected COFF.
                    if write_coff {
                        for index in &mut section.associated {
                            *index = sections[*index].mapping as usize;
                        }
                    } else {
                        section.associated = Vec::new();
                    }
                    for (relocation, view) in section.relocations.iter_mut().zip(self.selected_relocations(row)) {
                        if let Some(target) = self.slot_target(view) {
                            relocation.target = target;
                        } else if let Target::Section(index, _) = &mut relocation.target {
                            *index = view.section as usize;
                        }
                    }
                    Some(section)
                }).collect();
                let definitions = declared.into_iter().zip(symbols).filter_map(|(symbol, row)| {
                    if row.keep == 0 { return None; }
                    let mut definition = symbol.definition;
                    definition.section = row.section as usize;
                    Some(definition)
                }).collect();
                if let Some(frames) = &self.frames {
                    let mut index = row.frame_start as usize;
                    stack.retain_mut(|frame| {
                        let view = frames[index];
                        index += 1;
                        if view.keep == 0 { return false; }
                        if let Some((section, _)) = &mut frame.coff {
                            *section = view.mapping as usize;
                        }
                        true
                    });
                } else {
                    stack = Vec::new();
                }
                let library = match kind {
                    Kind::Library { header, binding, prefix } => Some(LibraryBinding { header, binding, prefix }),
                    _ => None,
                };
                Selected { name, library, sections, definitions, archive, stack }
            }
            Source::Borrowed(object)=>Selected{
                name:object.name.clone(),library:None,
                sections: object.sections.iter().zip(sections).filter_map(|(section, &row)| {
                    if row.live == 0 { return None; }
                    let relocations = section.relocations.iter().zip(self.selected_relocations(row))
                        .map(|(relocation, view)| {
                            let target = self.slot_target(view).unwrap_or_else(|| match &relocation.target {
                                Target::Section(_, value) => Target::Section(view.section as usize, *value),
                                Target::Symbol(name) => Target::Symbol(name.clone()),
                                Target::Import(import) => Target::Import(import.clone()),
                            });
                            object::Relocation { offset: relocation.offset, kind: relocation.kind, target }
                        }).collect();
                    Some(object::Section {
                        name: section.name.clone(),
                        bytes: section.bytes.as_ref().map(borrowed),
                        flags: section.flags,
                        associated: Vec::new(),
                        layout: None,
                        relocations,
                    })
                }).collect(),
                definitions: object.symbols.iter().zip(symbols).filter(|(_, row)| row.keep != 0)
                    .map(|(symbol, row)| {
                        let mut definition = symbol.definition.clone();
                        definition.section = row.section as usize;
                        definition
                    }).collect(),
                archive: None, stack: Vec::new(),
            },
        };
        if row.keep_archive==0{object.archive=None;}else if let Some(archive)=&mut object.archive{
            let mut index=0;archive.exports.retain(|_|{let keep=self.exports[row.export_start as usize+index].keep!=0;index+=1;keep});
        }
        object
    }
}
unsafe extern "C" fn extract<Payload: ReadSection>(
    context: *mut c_void, view: *mut View, library: u64, name: Bytes,
) -> AppendStatus {
    let context = unsafe { &mut *context.cast::<Context<'_, Payload>>() };
    let result = context.libraries[library as usize].extract(text(&name), context.frames.is_some());
    context.append_result(unsafe { &mut *view }, result)
}
unsafe extern "C" fn native_archive<Payload>(context: *mut c_void, index: u64) -> u64 {
    let context = unsafe { &mut *context.cast::<Context<'_, Payload>>() };
    let result = Library::native(context.sources[index as usize].archive().unwrap().bytes.clone());
    match result {
        Ok(library) => {
            context.libraries.push(library);
            context.libraries.len() as u64
        }
        Err(error) => {
            context.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn slot<Payload>(context:*mut c_void,view:*mut View,name:Bytes)->Bytes {
    let context=unsafe{&mut *context.cast::<Context<'_, Payload>>()};
    let work = context.native.as_mut().unwrap();
    work.slots.push(resources::import_symbol(crate::application::RUNTIME_LIBRARY, text(&name)));
    work.locals.push(MaybeUninit::uninit());
    context.nodes.push(MaybeUninit::uninit());
    // B initializes the slot and its tree node through these refreshed pointers.
    unsafe {
        (*view).locals = work.locals.as_mut_ptr().cast();
        (*view).nodes = context.nodes.as_mut_ptr().cast();
    }
    Bytes::new(work.slots.last().unwrap())
}
unsafe extern "C" fn binding<Payload: ReadSection>(
    context: *mut c_void, view: *mut View, request: ResourceRequest,
) -> AppendStatus {
    let context=unsafe{&mut *context.cast::<Context<'_, Payload>>()};
    let runtime=unsafe{std::slice::from_raw_parts(request.runtime,request.runtime_count as usize)};
    let locals=unsafe{std::slice::from_raw_parts(request.locals,request.local_count as usize)};
    // B activates archives from live sections. Borrow those sources in their original order.
    let archives = context.objects.iter().zip(&context.sources)
        .filter(|(object, _)| object.active_archive != 0)
        .map(|(_, source)| source.archive().unwrap());
    let result = resources::resources(
        archives, request.types as u32, request.programs as u32,
        runtime.iter().filter(|row| row.used != 0).map(|row| text(&row.name)),
        locals.iter().map(|row| (text(&row.slot), text(&row.name))),
        Some(crate::resource::ABI),
    );
    // The B selector requests generated bindings only for a native image.
    let result = result.map(|code| code.map(|code| Payload::from_generated(code.into_object())));
    context.append_result(unsafe { &mut *view }, result)
}
pub(super) fn entry<'a, Payload: ReadSection>(
    inputs: Inputs<'a, Payload>, stack_output: bool, borrowed: impl Fn(&'a Vec<u8>) -> Payload,
) -> Result<Selection<Payload>> {
    select(inputs, Roots::Entry { stack_output }, borrowed)
}
pub(super) fn select<'a, Payload: ReadSection>(
    inputs: Inputs<'a, Payload>, roots: Roots<'_>,
    borrowed: impl Fn(&'a Vec<u8>) -> Payload,
) -> Result<Selection<Payload>> {
    let(native_mode,types,programs,roots,stack_output)=match roots{
        Roots::Entry { stack_output }=>(0,0,0,&[][..],stack_output),
        Roots::Symbols(roots,types,programs)=>(1,u64::from(types),u64::from(programs),roots,false),
    };
    let mut context=Context::new(inputs, stack_output, native_mode != 0);
    let params=Params{native_mode,types,programs,roots:roots.as_ptr(),root_count:roots.len() as u64,runtime_library:Bytes::new(crate::application::RUNTIME_LIBRARY)};
    let input=context.view();
    let result=unsafe{choose(input,params,std::ptr::from_mut(&mut context).cast(), extract::<Payload>, native_archive::<Payload>, binding::<Payload>, slot::<Payload>)};
    match result.error{
        0=>{},
        1=>return Err("原生链接需要且只能有一个实际应用或算法库入口对象".into()),
        2=>return Err(format!("重复定义模块符号 {}",text(&result.name)).into()),
        3=>return Err(format!("未定义模块符号 {}",text(&result.name)).into()),
        4=>return Err(format!("模块接口不匹配 {}；调用方与定义须使用相同的类型、布局、访问及函数事实",text(&result.name)).into()),
        5=>return Err(format!("静态库目录选中的模块 {} 未定义请求符号 {}",text(&result.owner),text(&result.name)).into()),
        6=>return Err(format!("模块资源缺少所属CPU入口 {}",text(&result.name)).into()),
        7=>return Err(context.error.take().unwrap()),
        8=>return Err(format!("模块 {} 的实际引用缺少导入合同 {}",text(&result.owner),text(&result.name)).into()),
        code=>return Err(format!("原生链接选择失败 {code}").into()),
    }
    if native_mode==0{context.sources.swap(0,result.entry as usize);context.objects.swap(0,result.entry as usize);}
    let write_coff = !stack_output
        && matches!(result.plan.product, EntryProduct::HostApplication | EntryProduct::Dll);
    let sources=std::mem::take(&mut context.sources);let mut output=Vec::new();
    for(source,row)in sources.into_iter().zip(&context.objects){if row.selected!=0{output.push(context.materialize(source,*row,&borrowed,write_coff));}}
    Ok(Selection { objects: output, plan: result.plan })
}
