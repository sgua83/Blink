//! B chooses names and visibility; the original declaration maps keep their storage.
use crate::frontend::NameResolution;
use std::ffi::c_void;
#[derive(Clone,Copy)]
#[repr(C)]
struct Bytes { bytes:*const u8, length:u64 }
impl Bytes {
    fn new(value:&str)->Self {Self{bytes:value.as_ptr(),length:value.len()as u64}}
    unsafe fn text<'a>(self)->&'a str {
        unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes,self.length as usize))}
    }
}
#[repr(C)]
struct Hit { name:Bytes, owner:Bytes, public:u64, present:u64 }
#[repr(C)]
struct Resolution { name:Bytes, count:u64 }
pub(crate) struct Declaration<'a> { pub name:&'a str, pub owner:&'a str, pub public:bool }
unsafe extern "C" {
    #[link_name = "bootstrap.names::order_imports"]
    fn order_imports(names: *mut Bytes, count: u64) -> u64;
    #[link_name="bootstrap.names::resolve"]
    fn choose(name:Bytes,module:Bytes,imports:*const Bytes,count:u64,candidate:*mut u8,context:*const c_void,
        lookup:unsafe extern "C" fn(*const c_void,Bytes)->Hit)->Resolution;
}
unsafe extern "C" fn lookup<'a,F:Fn(&str)->Option<Declaration<'a>>>(context:*const c_void,name:Bytes)->Hit {
    let lookup=unsafe{&*context.cast::<F>()};
    match lookup(unsafe{name.text()}) {
        Some(declaration)=>Hit{name:Bytes::new(declaration.name),owner:Bytes::new(declaration.owner),public:u64::from(declaration.public),present:1},
        None=>Hit{name:Bytes::new(""),owner:Bytes::new(""),public:0,present:0},
    }
}
pub(crate) fn resolve<'a,F:Fn(&str)->Option<Declaration<'a>>>(name:&str,module:&str,imports:&[String],declarations:F)->NameResolution {
    let longest=imports.iter().map(String::len).chain(std::iter::once(module.len())).max().unwrap();
    // B initializes only the candidate bytes it reads. u8 storage has no drop work.
    let mut candidate=Vec::<u8>::with_capacity(longest+2+name.len());
    let imports=imports.iter().map(|name|Bytes::new(name)).collect::<Vec<_>>();
    let result=unsafe{choose(Bytes::new(name),Bytes::new(module),imports.as_ptr(),imports.len()as u64,
        candidate.as_mut_ptr(),&declarations as *const F as *const c_void,lookup::<F>)};
    match result.count {
        0=>NameResolution::Missing,
        1=>NameResolution::Unique(unsafe{result.name.text()}.to_owned()),
        _=>NameResolution::Ambiguous,
    }
}


pub(crate) fn module_imports(
    interface: &crate::syntax::Module,
    implementation: Option<&crate::syntax::Module>,
) -> std::sync::Arc<[String]> {
    let mut names = interface.imports()
        .chain(implementation.into_iter().flat_map(crate::syntax::Module::imports))
        .map(Bytes::new)
        .collect::<Vec<_>>();
    // B rearranges borrowed descriptors in this call. Both syntax owners stay live.
    let count = unsafe { order_imports(names.as_mut_ptr(), names.len() as u64) };
    names.truncate(count as usize);
    names.into_iter().map(|name| unsafe { name.text() }.to_owned()).collect()
}
