//! 链接期资源基址和工厂地址；只有静态数据与标准重定位，没有运行期注册表。
use super::Result;
use std::borrow::Borrow;
use crate::object::{Archive, Definition, Kind, Object, Relocation, Section, Symbol, Target};
#[derive(Clone,Copy)]
#[repr(C)]
pub(super) struct ImportBytes {pub(super) bytes:*const u8,pub(super) length:u64}
impl ImportBytes {pub(super) fn new(text:&str)->Self{Self{bytes:text.as_ptr(),length:text.len() as u64}}}
unsafe extern "C" {
    #[link_name="bootstrap.bindings::import_name_size"]
    fn binding_import_name_size(dll:ImportBytes,name:ImportBytes)->u64;
    #[link_name="bootstrap.bindings::import_name"]
    fn binding_import_name(dll:ImportBytes,name:ImportBytes,output:*mut u8);
}
pub(super) fn import_symbol(dll:&str,name:&str)->String {
    let dll=ImportBytes::new(dll);let name=ImportBytes::new(name);
    let length=unsafe{binding_import_name_size(dll,name)} as usize;
    let mut output=Vec::with_capacity(length);
    unsafe{binding_import_name(dll,name,output.as_mut_ptr());output.set_len(length);String::from_utf8_unchecked(output)}
}
#[derive(Clone,Copy)]
#[repr(C)]
struct ResourceModule {
    prefix:ImportBytes,abi:ImportBytes,types:u64,programs:u64,type_value:u64,program_value:u64,
    factory_offset:u64,type_offset:u64,program_offset:u64,
}
#[derive(Clone,Copy)]
#[repr(C)]
struct ResourceLocal {slot:ImportBytes,name:ImportBytes,value:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct ResourceRuntime {name:ImportBytes,value:u64,relocation:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct ResourceInput {
    modules:*mut ResourceModule,module_count:u64,locals:*mut ResourceLocal,local_count:u64,
    runtime:*mut ResourceRuntime,runtime_count:u64,root_abi:ImportBytes,has_root_abi:u64,
    types:u64,programs:u64,order:*mut u64,
}
#[derive(Clone,Copy)]
#[repr(C)]
struct ResourceOutput {data:*mut u8,text:*mut u8}
#[derive(Clone,Copy)]
#[repr(C)]
struct ResourcePlan {
    data:u64,text:u64,data_flags:u64,text_flags:u64,error:u64,at:u64,
    factory_suffix:u64,type_suffix:u64,program_suffix:u64,
}
unsafe extern "C" {
    #[link_name="bootstrap.native_resources::prepare"]
    fn resource_prepare(input:ResourceInput)->ResourcePlan;
    #[link_name="bootstrap.native_resources::encode"]
    fn resource_encode(input:ResourceInput,output:ResourceOutput,plan:ResourcePlan);
    #[link_name="bootstrap.native_resources::encode_module"]
    fn resource_encode_module(row:ResourceModule,data:*mut u8,factory:*mut u8,type_name:*mut u8,program_name:*mut u8);
}
pub(super) struct Generated {
    pub sections: Vec<Section>,
    pub definitions: Vec<Definition>,
}

impl Generated {
    pub const NAME: &str = "@blink.resources@";

    pub fn into_object(self) -> Object {
        let imports = self.sections.iter().flat_map(|section| &section.relocations)
            .filter_map(|relocation| match &relocation.target {
                Target::Symbol(name) => Some((name.clone(), Vec::new())),
                _ => None,
            }).collect();
        let symbols = self.definitions.into_iter().map(|definition| Symbol {
            definition, contract: Vec::new(),
        }).collect();
        Object {
            name: Self::NAME.into(),
            kind: Kind::Module { interface: "".into(), requirements: Default::default(), stack_bytes: 0 },
            sections: self.sections.into_iter().map(Section::initialized).collect(),
            symbols,
            imports,
            archive: None,
            stack: Vec::new(),
        }
    }
}

pub(super) fn resources<'archive, 'name>(
    archives: impl IntoIterator<Item = impl Borrow<&'archive Archive>>,
    types: u32,
    programs: u32,
    runtime: impl IntoIterator<Item = &'name str>,
    local_imports: impl IntoIterator<Item = (&'name str, &'name str)>,
    root_abi: Option<&[u8]>,
) -> Result<Option<Generated>> {
    let borrowed=|bytes:&[u8]|ImportBytes{bytes:bytes.as_ptr(),length:bytes.len() as u64};
    let empty=ImportBytes{bytes:std::ptr::null(),length:0};
    let mut modules=archives.into_iter().filter_map(|archive| (*archive.borrow()).resources.as_ref()).map(|row|ResourceModule{
        prefix:ImportBytes::new(&row.prefix),abi:borrowed(&row.abi),types:row.types.into(),programs:row.programs.into(),
        type_value:0,program_value:0,factory_offset:0,type_offset:0,program_offset:0,
    }).collect::<Vec<_>>();
    let mut locals=local_imports.into_iter().map(|(slot,name)|ResourceLocal{slot:ImportBytes::new(slot),name:ImportBytes::new(name),value:0}).collect::<Vec<_>>();
    let mut runtime=runtime.into_iter().map(|name|ResourceRuntime{name:ImportBytes::new(name),value:0,relocation:0}).collect::<Vec<_>>();
    let mut order=vec![0;modules.len()];
    let input=ResourceInput{
        modules:modules.as_mut_ptr(),module_count:modules.len() as u64,locals:locals.as_mut_ptr(),local_count:locals.len() as u64,
        runtime:runtime.as_mut_ptr(),runtime_count:runtime.len() as u64,root_abi:root_abi.map(borrowed).unwrap_or(empty),
        has_root_abi:root_abi.is_some() as u64,types:types.into(),programs:programs.into(),order:order.as_mut_ptr(),
    };
    let text=|row:ImportBytes|unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(row.bytes,row.length as usize))}.to_owned();
    let plan=unsafe{resource_prepare(input)};
    match plan.error {
        0=>{},1=>return Err("模块资源 C ABI 不一致，须重编为同一实际资源 ABI".into()),
        2=>return Err(format!("重复的模块资源定义 {}",text(modules[usize::try_from(plan.at)?].prefix)).into()),
        3=>return Err("模块类型索引超出原生 ABI".into()),4=>return Err("模块程序索引超出原生 ABI".into()),
        5=>return Err("模块资源字段位置超出COFF格式".into()),code=>return Err(format!("模块资源绑定失败 {code}").into()),
    }
    if plan.data==0{return Ok(None);}
    let data_length=usize::try_from(plan.data)?;let text_length=usize::try_from(plan.text)?;
    let mut data=Vec::with_capacity(data_length);let mut code=Vec::with_capacity(text_length);
    unsafe{resource_encode(input,ResourceOutput{data:data.as_mut_ptr(),text:code.as_mut_ptr()},plan);data.set_len(data_length);code.set_len(text_length);}
    let finish=|mut bytes:Vec<u8>,length:usize|{unsafe{bytes.set_len(length);String::from_utf8_unchecked(bytes)}};
    let mut definitions=vec![
        Definition{name:"__blink_module_count".into(),section:0,value:0,function:false},
        Definition{name:"__blink_module_factories".into(),section:0,value:8,function:false},
    ];
    let mut relocations=Vec::new();
    for row in modules {
        let factory_length = usize::try_from(row.prefix.length + plan.factory_suffix)?;
        let type_length = usize::try_from(row.prefix.length + plan.type_suffix)?;
        let program_length = usize::try_from(row.prefix.length + plan.program_suffix)?;
        let mut factory = Vec::with_capacity(factory_length);
        let mut type_name = Vec::with_capacity(type_length);
        let mut program_name = Vec::with_capacity(program_length);
        unsafe {
            resource_encode_module(row, data.as_mut_ptr(), factory.as_mut_ptr(), type_name.as_mut_ptr(), program_name.as_mut_ptr());
        }
        relocations.push(Relocation{offset:usize::try_from(row.factory_offset)?,kind:1,target:Target::Symbol(finish(factory,factory_length))});
        definitions.push(Definition{name:finish(type_name,type_length),section:0,value:u32::try_from(row.type_offset)?,function:false});
        definitions.push(Definition{name:finish(program_name,program_length),section:0,value:u32::try_from(row.program_offset)?,function:false});
    }
    for row in locals{
        let name=text(row.name);
        definitions.push(Definition{name:text(row.slot),section:0,value:u32::try_from(row.value)?,function:false});
        relocations.push(Relocation{offset:usize::try_from(row.value)?,kind:1,target:Target::Symbol(name)});
    }
    let mut sections=vec![Section{associated:Vec::new(),layout:None,name:".rdata".into(),bytes:data,flags:u32::try_from(plan.data_flags)?,relocations}];
    if plan.text!=0{
        let mut relocations=Vec::new();
        for row in runtime{
            let name=text(row.name);
            definitions.push(Definition{name:name.clone(),section:1,value:u32::try_from(row.value)?,function:true});
            relocations.push(Relocation{offset:usize::try_from(row.relocation)?,kind:4,target:Target::Import(crate::object::Import{dll:crate::resource::RUNTIME_LIBRARY.into(),name})});
        }
        sections.push(Section{associated:Vec::new(),layout:None,name:".text".into(),bytes:code,flags:u32::try_from(plan.text_flags)?,relocations});
    }
    Ok(Some(Generated { sections, definitions }))
}
