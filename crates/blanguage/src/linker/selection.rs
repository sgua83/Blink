//! B owns this link's definition tree, section queue, resource closure and remapping.
//! Rust keeps the original objects/libraries and only hands their current borrowed views across the synchronous call.
use super::{Inputs,Result,Roots,Source,resources};
use crate::object::{self,Object,Kind,Target,Library};
use resources::ImportBytes as Bytes;
use std::{ffi::c_void, marker::PhantomData};
#[derive(Clone,Copy)]#[repr(C)]
struct ObjectView {
    kind:u64,name:Bytes,section_start:u64,section_count:u64,symbol_start:u64,symbol_count:u64,
    import_start:u64,import_count:u64,export_start:u64,export_count:u64,code_start:u64,code_count:u64,
    frame_start:u64,frame_count:u64,prefix:Bytes,archive:u64,has_resources:u64,selected:u64,resources:u64,keep_archive:u64,borrowed:u64,
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
struct SelectionResult {error:u64,name:Bytes,owner:Bytes,entry:u64}
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
struct Context<'a> {
    sources:Vec<Source<'a>>,libraries:Vec<Library>,objects:Vec<ObjectView>,sections:Vec<SectionView>,
    relocations:Vec<RelocationView>,associated:Vec<u64>,symbols:Vec<SymbolView>,imports:Vec<ImportView>,
    exports:Vec<ExportView>,code:Vec<Bytes>,frames:Vec<FrameView>,nodes:Vec<[u64;9]>,pending:Vec<u64>,
    locals:Vec<LocalView>,runtime:Vec<Bytes>,archives:Vec<u64>,slots:Vec<String>,error:Option<Box<dyn std::error::Error>>,
}
impl<'a> Context<'a> {
    fn new(inputs: Inputs<'a>)->Self {
        let Inputs { sources, libraries } = inputs;
        let mut context=Self{sources,libraries,objects:Vec::new(),sections:Vec::new(),relocations:Vec::new(),associated:Vec::new(),
            symbols:Vec::new(),imports:Vec::new(),exports:Vec::new(),code:Vec::new(),frames:Vec::new(),nodes:Vec::new(),pending:Vec::new(),
            locals:Vec::new(),runtime:Vec::new(),archives:Vec::new(),slots:Vec::new(),error:None};
        for index in 0..context.sources.len(){context.describe(index);}
        context.grow();context
    }
    fn describe(&mut self,index:usize) {
        let object=&*self.sources[index];let empty=Bytes::new("");let borrowed_source=matches!(self.sources[index],Source::Borrowed(_));
        let mut row=ObjectView{
            kind:match object.kind{Kind::Module{..}=>0,Kind::Application{..}=>1,Kind::Library{..}=>2},name:Bytes::new(&object.name),
            section_start:self.sections.len() as u64,section_count:object.sections.len() as u64,symbol_start:self.symbols.len() as u64,symbol_count:object.symbols.len() as u64,
            import_start:self.imports.len() as u64,import_count:object.imports.len() as u64,export_start:self.exports.len() as u64,export_count:0,
            code_start:self.code.len() as u64,code_count:0,frame_start:self.frames.len() as u64,frame_count:0,prefix:empty,
            archive:object.archive.is_some() as u64,has_resources:0,selected:0,resources:0,keep_archive:0,borrowed:borrowed_source as u64,
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
        self.symbols.extend(object.symbols.iter().map(|symbol|SymbolView{name:Bytes::new(&symbol.name),contract:borrowed(&symbol.contract),owner:index as u64,section:symbol.section as u64,keep:0}));
        self.imports.extend(object.imports.iter().map(|(name,contract)|ImportView{name:Bytes::new(name),contract:borrowed(contract)}));
        if let Some(archive)=&object.archive{
            row.export_count=archive.exports.len() as u64;self.exports.extend(archive.exports.iter().map(|name|ExportView{name:Bytes::new(name),keep:1}));
            if let Some(resource)=&archive.resources{
                row.has_resources=1;row.prefix=Bytes::new(&resource.prefix);row.code_count=resource.code.len() as u64;
                self.code.extend(resource.code.iter().map(|name|Bytes::new(name)));
            }
        }
        // Borrowed native code has no compiler-report consumer.
        if !borrowed_source{
            row.frame_count=object.stack.len() as u64;
            self.frames.extend(object.stack.iter().map(|frame|FrameView{section:frame.coff.map_or(0,|x|x.0 as u64),present:frame.coff.is_some() as u64,keep:0,mapping:0}));
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
            exports:self.exports.as_mut_ptr(),code:self.code.as_mut_ptr(),frames:self.frames.as_mut_ptr(),nodes:self.nodes.as_mut_ptr(),pending:self.pending.as_mut_ptr(),
            locals:self.locals.as_mut_ptr(),runtime:self.runtime.as_mut_ptr(),archives:self.archives.as_mut_ptr(),library_count:self.libraries.len() as u64}
    }
    fn append(&mut self,object:Object)->u64 {
        let index=self.sources.len();self.sources.push(Source::Owned(object));self.describe(index);self.grow();index as u64+1
    }
    fn reply(&mut self,result:Result<Option<Object>>)->Reply {
        match result{
            Ok(object)=>{let object=object.map_or(0,|object|self.append(object));Reply{view:self.view(),object,error:0}},
            Err(error)=>{self.error=Some(error);Reply{view:self.view(),object:0,error:1}},
        }
    }
    fn section(&self,section:&mut object::Section,row:SectionView) {
        section.associated=self.associated[row.associated_start as usize..(row.associated_start+row.associated_count) as usize].iter().map(|&i|i as usize).collect();
        for(relocation,view)in section.relocations.iter_mut().zip(&self.relocations[row.relocation_start as usize..(row.relocation_start+row.relocation_count) as usize]){
            if let Target::Section(index,_)=&mut relocation.target{*index=view.section as usize;}
            if view.rewrite!=0{relocation.target=Target::Symbol(text(&self.locals[view.rewrite as usize-1].slot).to_owned());}
        }
    }
    fn materialize(&self,source:Source<'_>,row:ObjectView)->Object {
        let sections=&self.sections[row.section_start as usize..(row.section_start+row.section_count) as usize];
        let symbols=&self.symbols[row.symbol_start as usize..(row.symbol_start+row.symbol_count) as usize];
        let mut object=match source {
            Source::Owned(mut object)=>{
                object.sections=object.sections.into_iter().zip(sections).filter_map(|(mut section,&row)|{
                    if row.live==0{return None;}self.section(&mut section,row);Some(section)
                }).collect();
                let mut index=0;object.symbols.retain_mut(|symbol|{let row=symbols[index];index+=1;symbol.section=row.section as usize;row.keep!=0});
                let mut index=0;object.stack.retain_mut(|frame|{let view=self.frames[row.frame_start as usize+index];index+=1;
                    if let Some((_,start))=frame.coff{frame.coff=Some((view.mapping as usize,start));}view.keep!=0
                });object
            }
            Source::Borrowed(object)=>Object{
                name:object.name.clone(),kind:object.kind.clone(),
                sections:object.sections.iter().zip(sections).filter_map(|(section,&row)|{
                    if row.live==0{return None;}
                    let mut section=object::Section{name:section.name.clone(),bytes:section.bytes.clone(),flags:section.flags,associated:Vec::new(),layout:None,
                        relocations:section.relocations.iter().map(|relocation|object::Relocation{offset:relocation.offset,kind:relocation.kind,target:match &relocation.target{
                            Target::Section(index,value)=>Target::Section(*index,*value),Target::Symbol(name)=>Target::Symbol(name.clone()),Target::Import(import)=>Target::Import(import.clone()),
                        }}).collect()};
                    self.section(&mut section,row);Some(section)
                }).collect(),
                symbols:object.symbols.iter().zip(symbols).filter(|(_,row)|row.keep!=0).map(|(symbol,row)|object::Symbol{
                    name:symbol.name.clone(),section:row.section as usize,value:symbol.value,contract:symbol.contract.clone(),function:symbol.function,
                }).collect(),imports:object.imports.clone(),archive:None,stack:Vec::new(),
            },
        };
        if row.keep_archive==0{object.archive=None;}else if let Some(archive)=&mut object.archive{
            let mut index=0;archive.exports.retain(|_|{let keep=self.exports[row.export_start as usize+index].keep!=0;index+=1;keep});
        }
        object
    }
}
unsafe extern "C" fn extract(context:*mut c_void,library:u64,name:Bytes)->Reply {
    let context=unsafe{&mut *context.cast::<Context<'_>>()};
    let result=context.libraries[library as usize].extract(text(&name));context.reply(result)
}
unsafe extern "C" fn native_archive(context:*mut c_void,index:u64)->Reply {
    let context=unsafe{&mut *context.cast::<Context<'_>>()};
    let result=Library::native(context.sources[index as usize].archive.as_ref().unwrap().bytes.clone());
    match result {Ok(library)=>{context.libraries.push(library);context.reply(Ok(None))},Err(error)=>context.reply(Err(error))}
}
unsafe extern "C" fn slot(context:*mut c_void,name:Bytes)->Bytes {
    let context=unsafe{&mut *context.cast::<Context<'_>>()};
    context.slots.push(resources::import_symbol(crate::application::RUNTIME_LIBRARY,text(&name)));Bytes::new(context.slots.last().unwrap())
}
unsafe extern "C" fn binding(context:*mut c_void,request:ResourceRequest)->Reply {
    let context=unsafe{&mut *context.cast::<Context<'_>>()};
    let archives=unsafe{std::slice::from_raw_parts(request.archives,request.archive_count as usize)};
    let runtime=unsafe{std::slice::from_raw_parts(request.runtime,request.runtime_count as usize)};
    let locals=unsafe{std::slice::from_raw_parts(request.locals,request.local_count as usize)};
    let archives=archives.iter().map(|&index|context.sources[index as usize].archive.as_ref().unwrap()).collect::<Vec<_>>();
    let result=resources::resources(&archives,request.types as u32,request.programs as u32,runtime.iter().map(text),locals.iter().map(|row|(text(&row.slot),text(&row.name))),Some(crate::resource::ABI));
    context.reply(result)
}
pub(super) fn select(inputs: Inputs<'_>, roots: Roots<'_>)->Result<Vec<Object>> {
    let(native_mode,types,programs,roots)=match roots{
        Roots::Entry=>(0,0,0,&[][..]),
        Roots::Symbols(roots,types,programs)=>(1,u64::from(types),u64::from(programs),roots),
    };
    let runtime=crate::resource::MODULE_RUNTIME_SYMBOLS.iter().map(|name|Bytes::new(name)).collect::<Vec<_>>();
    let params=Params{native_mode,types,programs,roots:roots.as_ptr(),root_count:roots.len() as u64,runtime_library:Bytes::new(crate::application::RUNTIME_LIBRARY),runtime_symbols:runtime.as_ptr(),runtime_symbol_count:runtime.len() as u64};
    let mut context=Context::new(inputs);let input=context.view();
    let result=unsafe{choose(input,params,&mut context as *mut Context<'_> as *mut c_void,extract,native_archive,binding,slot)};
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
    for(source,row)in sources.into_iter().zip(&context.objects){if row.selected!=0{output.push(context.materialize(source,*row));}}
    Ok(output)
}
