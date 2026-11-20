//! B owns this link's definition tree, section queue, resource closure and remapping.
//! Rust keeps the original objects/libraries and only hands their current borrowed views across the synchronous call.
use super::{EntryPlan, Inputs, LibraryBinding, Result, Roots, Selected, Selection, Source, resources};
use crate::object::{self, Object, Kind, Target, Library, ReadSection};
use resources::ImportBytes as Bytes;
use std::{ffi::c_void, marker::PhantomData};
#[derive(Clone,Copy)]#[repr(C)]
struct ObjectView {
    kind:u64,name:Bytes,section_start:u64,section_count:u64,symbol_start:u64,symbol_count:u64,
    import_start:u64,import_count:u64,export_start:u64,export_count:u64,code_start:u64,code_count:u64,
    frame_start:u64,frame_count:u64,prefix:Bytes,archive:u64,has_resources:u64,selected:u64,resources:u64,keep_archive:u64,borrowed:u64,stack_bytes:u64,
}
#[derive(Clone,Copy)]#[repr(C)]
struct SectionView {owner:u64,flags:u64,associated_start:u64,associated_count:u64,relocation_start:u64,relocation_count:u64,live:u64,mapping:u64}
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
#[derive(Clone,Copy)]#[repr(C)]
pub(crate) struct RootView<'a> {
    name: Bytes,
    contract: Bytes,
    owner: PhantomData<&'a [u8]>,
}
impl<'a> RootView<'a> {
    pub(crate) fn new(name: &'a str, contract: &'a [u8]) -> Self {
        Self { name: Bytes::new(name), contract: borrowed(contract), owner: PhantomData }
    }
}
#[derive(Clone,Copy)]#[repr(C)]
struct View {
    objects:*mut ObjectView,object_count:u64,sections:*mut SectionView,section_count:u64,
    relocations:*mut RelocationView,associated:*mut u64,symbols:*mut SymbolView,imports:*mut ImportView,
    exports:*mut ExportView,code:*mut Bytes,frames:*mut FrameView,nodes:*mut [u64;9],pending:*mut u64,
    locals:*mut LocalView,runtime:*mut Bytes,archives:*mut u64,library_count:u64,
}
#[derive(Clone,Copy)]#[repr(C)]
struct Params<'a> {native_mode:u64,roots:*const RootView<'a>,root_count:u64,types:u64,programs:u64,runtime_library:Bytes,runtime_symbols:*const Bytes,runtime_symbol_count:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct Reply {view:View,object:u64,error:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct ResourceRequest {archives:*const u64,archive_count:u64,runtime:*const Bytes,runtime_count:u64,locals:*const LocalView,local_count:u64,types:u64,programs:u64}
#[derive(Clone,Copy)]#[repr(C)]
struct SelectionResult {error:u64,name:Bytes,owner:Bytes,entry:u64,plan:EntryPlan}
type Extract=unsafe extern "C" fn(*mut c_void,u64,Bytes)->Reply;
type NativeArchive=unsafe extern "C" fn(*mut c_void,u64)->Reply;
type ResourceBinding=unsafe extern "C" fn(*mut c_void,ResourceRequest)->Reply;
type Slot=unsafe extern "C" fn(*mut c_void,Bytes)->Bytes;
unsafe extern "C" {
    #[link_name="bootstrap.native_select::choose"]
    fn choose(input:View,params:Params<'_>,context:*mut c_void,extract:Extract,native_archive:NativeArchive,resources:ResourceBinding,slot:Slot)->SelectionResult;
}
fn borrowed(bytes:&[u8])->Bytes {Bytes{bytes:bytes.as_ptr(),length:bytes.len() as u64}}
fn text(bytes:&Bytes)->&str {unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytes.bytes,bytes.length as usize))}}
struct Context<'a, Payload> {
    sources:Vec<Source<'a, Payload>>,libraries:Vec<Library>,objects:Vec<ObjectView>,sections:Vec<SectionView>,
    relocations:Vec<RelocationView>,associated:Vec<u64>,symbols:Vec<SymbolView>,imports:Vec<ImportView>,
    exports:Vec<ExportView>,code:Vec<Bytes>,frames:Option<Vec<FrameView>>,nodes:Vec<[u64;9]>,pending:Vec<u64>,
    locals:Vec<LocalView>,runtime:Vec<Bytes>,archives:Vec<u64>,slots:Vec<String>,error:Option<Box<dyn std::error::Error>>,
}
impl<'a, Payload> Context<'a, Payload> {
    fn new(inputs: Inputs<'a, Payload>, stack_output: bool)->Self {
        let Inputs { sources, libraries } = inputs;
        let mut context=Self{sources,libraries,objects:Vec::new(),sections:Vec::new(),relocations:Vec::new(),associated:Vec::new(),
            symbols:Vec::new(),imports:Vec::new(),exports:Vec::new(),code:Vec::new(),frames:stack_output.then(Vec::new),nodes:Vec::new(),pending:Vec::new(),
            locals:Vec::new(),runtime:Vec::new(),archives:Vec::new(),slots:Vec::new(),error:None};
        for index in 0..context.sources.len(){context.describe(index);}
        context.grow();context
    }
    fn describe(&mut self,index:usize) {
        let object=&*self.sources[index];let empty=Bytes::new("");let borrowed_source=matches!(self.sources[index],Source::Borrowed(_));
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
            archive:object.archive.is_some() as u64,has_resources:0,selected:0,resources:0,keep_archive:0,borrowed:borrowed_source as u64,stack_bytes:u64::from(stack_bytes),
        };
        for section in &object.sections {
            self.sections.push(SectionView{owner:index as u64,flags:section.flags.into(),associated_start:self.associated.len() as u64,associated_count:section.associated.len() as u64,
                relocation_start:self.relocations.len() as u64,relocation_count:section.relocations.len() as u64,live:0,mapping:0});
            self.associated.extend(section.associated.iter().map(|&i|i as u64));
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
            row.export_count=archive.exports.len() as u64;self.exports.extend(archive.exports.iter().map(|name|ExportView{name:Bytes::new(name),keep:1}));
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
        self.nodes.resize(self.symbols.len()+2*self.relocations.len(),[0;9]);
        self.pending.resize(self.sections.len(),0);
        self.locals.resize(self.relocations.len(),LocalView{slot:Bytes::new(""),name:Bytes::new("")});
        self.runtime.resize(self.relocations.len(),Bytes::new(""));
        self.archives.resize(self.objects.len(),0);
    }
    fn view(&mut self)->View {
        View{objects:self.objects.as_mut_ptr(),object_count:self.objects.len() as u64,sections:self.sections.as_mut_ptr(),section_count:self.sections.len() as u64,
            relocations:self.relocations.as_mut_ptr(),associated:self.associated.as_mut_ptr(),symbols:self.symbols.as_mut_ptr(),imports:self.imports.as_mut_ptr(),
            exports:self.exports.as_mut_ptr(),code:self.code.as_mut_ptr(),frames:self.frames.as_mut().map_or(std::ptr::null_mut(), Vec::as_mut_ptr),nodes:self.nodes.as_mut_ptr(),pending:self.pending.as_mut_ptr(),
            locals:self.locals.as_mut_ptr(),runtime:self.runtime.as_mut_ptr(),archives:self.archives.as_mut_ptr(),library_count:self.libraries.len() as u64}
    }
    fn append(&mut self, object: Object<Payload>) -> u64 {
        let index=self.sources.len();self.sources.push(Source::Owned(object));self.describe(index);self.grow();index as u64+1
    }
    fn reply(&mut self, result: Result<Option<Object<Payload>>>) -> Reply {
        match result{
            Ok(object)=>{let object=object.map_or(0,|object|self.append(object));Reply{view:self.view(),object,error:0}},
            Err(error)=>{self.error=Some(error);Reply{view:self.view(),object:0,error:1}},
        }
    }
    fn section<Bytes>(&self,section:&mut object::Section<Bytes>,row:SectionView) {
        section.associated=self.associated[row.associated_start as usize..(row.associated_start+row.associated_count) as usize].iter().map(|&i|i as usize).collect();
        for(relocation,view)in section.relocations.iter_mut().zip(&self.relocations[row.relocation_start as usize..(row.relocation_start+row.relocation_count) as usize]){
            if let Target::Section(index,_)=&mut relocation.target{*index=view.section as usize;}
            if view.rewrite!=0{relocation.target=Target::Symbol(text(&self.locals[view.rewrite as usize-1].slot).to_owned());}
        }
    }
    fn materialize<Output>(
        &self, source: Source<'a, Payload>, row: ObjectView,
        owned: &impl Fn(Payload) -> Output,
        borrowed: &impl Fn(&'a Payload) -> Output,
    ) -> Selected<Output> {
        let sections=&self.sections[row.section_start as usize..(row.section_start+row.section_count) as usize];
        let symbols=&self.symbols[row.symbol_start as usize..(row.symbol_start+row.symbol_count) as usize];
        let mut object=match source {
            Source::Owned(object) => {
                let Object { name, kind, sections: input, symbols: declared, archive, mut stack, .. } = object;
                let sections = input.into_iter().zip(sections).filter_map(|(mut section, &row)| {
                    if row.live == 0 { return None; }
                    self.section(&mut section, row);
                    Some(section.map_bytes(owned))
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
                        if let Some((_, start)) = frame.coff {
                            frame.coff = Some((view.mapping as usize, start));
                        }
                        view.keep != 0
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
                sections:object.sections.iter().zip(sections).filter_map(|(section,&row)|{
                    if row.live==0{return None;}
                    let mut section=object::Section{name:section.name.clone(),bytes:borrowed(&section.bytes),flags:section.flags,associated:Vec::new(),layout:None,
                        relocations:section.relocations.iter().map(|relocation|object::Relocation{offset:relocation.offset,kind:relocation.kind,target:match &relocation.target{
                            Target::Section(index,value)=>Target::Section(*index,*value),Target::Symbol(name)=>Target::Symbol(name.clone()),Target::Import(import)=>Target::Import(import.clone()),
                        }}).collect()};
                    self.section(&mut section,row);Some(section)
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
unsafe extern "C" fn extract<Payload: ReadSection>(context:*mut c_void,library:u64,name:Bytes)->Reply {
    let context=unsafe{&mut *context.cast::<Context<'_, Payload>>()};
    let result=context.libraries[library as usize].extract(text(&name), context.frames.is_some());context.reply(result)
}
unsafe extern "C" fn native_archive<Payload>(context:*mut c_void,index:u64)->Reply {
    let context=unsafe{&mut *context.cast::<Context<'_, Payload>>()};
    let result=Library::native(context.sources[index as usize].archive.as_ref().unwrap().bytes.clone());
    match result {Ok(library)=>{context.libraries.push(library);context.reply(Ok(None))},Err(error)=>context.reply(Err(error))}
}
unsafe extern "C" fn slot<Payload>(context:*mut c_void,name:Bytes)->Bytes {
    let context=unsafe{&mut *context.cast::<Context<'_, Payload>>()};
    context.slots.push(resources::import_symbol(crate::application::RUNTIME_LIBRARY,text(&name)));Bytes::new(context.slots.last().unwrap())
}
unsafe extern "C" fn binding<Payload: ReadSection>(context:*mut c_void,request:ResourceRequest)->Reply {
    let context=unsafe{&mut *context.cast::<Context<'_, Payload>>()};
    let archives=unsafe{std::slice::from_raw_parts(request.archives,request.archive_count as usize)};
    let runtime=unsafe{std::slice::from_raw_parts(request.runtime,request.runtime_count as usize)};
    let locals=unsafe{std::slice::from_raw_parts(request.locals,request.local_count as usize)};
    let archives=archives.iter().map(|&index|context.sources[index as usize].archive.as_ref().unwrap()).collect::<Vec<_>>();
    let result=resources::resources(&archives,request.types as u32,request.programs as u32,runtime.iter().map(text),locals.iter().map(|row|(text(&row.slot),text(&row.name))),Some(crate::resource::ABI));
    // The B selector requests generated bindings only for a native image.
    let result = result.map(|code| code.map(|code| Payload::from_generated(code.into_object())));
    context.reply(result)
}
pub(super) fn entry<Payload: ReadSection + Clone>(
    inputs: Inputs<'_, Payload>, stack_output: bool,
) -> Result<Selection<Payload>> {
    select(inputs, Roots::Entry { stack_output }, std::convert::identity, Clone::clone)
}
pub(super) fn select<'a, Payload: ReadSection, Output>(
    inputs: Inputs<'a, Payload>, roots: Roots<'_>,
    owned: impl Fn(Payload) -> Output,
    borrowed: impl Fn(&'a Payload) -> Output,
) -> Result<Selection<Output>> {
    let(native_mode,types,programs,roots,stack_output)=match roots{
        Roots::Entry { stack_output }=>(0,0,0,&[][..],stack_output),
        Roots::Symbols(roots,types,programs)=>(1,u64::from(types),u64::from(programs),roots,false),
    };
    let runtime=crate::resource::MODULE_RUNTIME_SYMBOLS.iter().map(|name|Bytes::new(name)).collect::<Vec<_>>();
    let params=Params{native_mode,types,programs,roots:roots.as_ptr(),root_count:roots.len() as u64,runtime_library:Bytes::new(crate::application::RUNTIME_LIBRARY),runtime_symbols:runtime.as_ptr(),runtime_symbol_count:runtime.len() as u64};
    let mut context=Context::new(inputs, stack_output);let input=context.view();
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
    let sources=std::mem::take(&mut context.sources);let mut output=Vec::new();
    for(source,row)in sources.into_iter().zip(&context.objects){if row.selected!=0{output.push(context.materialize(source,*row,&owned,&borrowed));}}
    Ok(Selection { objects: output, plan: result.plan })
}
