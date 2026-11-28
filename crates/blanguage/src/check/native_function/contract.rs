//! The sole public contract projection. Checking retains its shared facts;
//! only normalized fields need temporary copies for the original encoder.
use super::*;

fn describe_value(value:&Value,normal:&mut bounds::Normalizer) {
    for(&reg,&(scalar,_))in value.regs.iter().zip(&value.ty.components){normal.scalar(ValueIdentity::Register(reg),scalar);}
}
fn describe_elements(elements:&application::ElementFacts,normal:&mut bounds::Normalizer) {
    use application::ElementFacts;
    match elements {
        ElementFacts::Known(value)=>describe(value,normal),
        ElementFacts::Common(a,b)|ElementFacts::Selected(_,a,b)=>{describe_elements(a,normal);describe_elements(b,normal);},
        _=>{},
    }
}
fn describe(value:&Returned,normal:&mut bounds::Normalizer) {
    match value {
        Returned::Unit=>{},Returned::Value(value)=>describe_value(value,normal),
        Returned::Dataset(data)=>{
            describe_value(&data.root,normal);describe_value(&data.length,normal);
            normal.dataset(data.root.identity(),data.length.identity());describe_elements(&data.elements,normal);
            if let application::DataLayout::Parameter{stride,offset,..}=&data.layout{describe_value(stride,normal);describe_value(offset,normal);}
        },
        Returned::OwnedValue(value)=>{describe_value(&value.value,normal);for value in value.payloads.iter().flatten(){describe(value,normal);}},
    }
}
fn value(value:&mut Value,normal:&mut bounds::Normalizer){normal.proof(&mut value.proof);}
fn elements(elements:&mut application::ElementFacts,normal:&mut bounds::Normalizer) {
    use application::ElementFacts;
    match elements {
        ElementFacts::Known(value)=>model(value,normal),
        ElementFacts::Input(when)|ElementFacts::Cleanup(when)=>*when=normal.condition(when),
        ElementFacts::Common(a,b)=>{self::elements(a,normal);self::elements(b,normal);},
        ElementFacts::Selected(when,a,b)=>{**when=normal.condition(when);self::elements(a,normal);self::elements(b,normal);},
        ElementFacts::Unknown=>{},
    }
}
fn model(model:&mut Returned,normal:&mut bounds::Normalizer) {
    match model {
        Returned::Unit=>{},Returned::Value(v)=>value(v,normal),
        Returned::Dataset(data)=>{
            value(&mut data.root,normal);value(&mut data.length,normal);
            data.residency.normalize_contract(normal);elements(&mut data.elements,normal);
            for borrow in data.prefixes.values_mut(){borrow.normalize_contract(normal);}
            if let application::DataLayout::Parameter{stride,offset,..}=&mut data.layout{value(stride,normal);value(offset,normal);}
        },
        Returned::OwnedValue(v)=>{value(&mut v.value,normal);for v in v.payloads.iter_mut().flatten(){self::model(v,normal);}},
    }
}
fn initialization(values:&mut [address::Initialization],normal:&mut bounds::Normalizer) {
    for value in values {
        match &mut value.source {
            address::InitializationSource::Position(origin)=>normal.origin(origin),
            address::InitializationSource::Task(borrow)=>borrow.normalize_contract(normal),
        }
        value.when=normal.condition(&value.when);
    }
}
type PublicAccess = view_contracts::Access;
type Slot = *mut std::ffi::c_void;
type Borrowed = *const std::ffi::c_void;

#[repr(C)]
struct AccessGroup {
    key: PublicAccess,
    condition: bounds::Condition,
}

#[repr(C)]
struct AccessGroupRow {
    links: [usize; 4],
    group: AccessGroup,
}

#[repr(C)]
struct AccessGroupHost {
    context: Slot,
    next: unsafe extern "C" fn(Slot) -> Borrowed,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    seed: unsafe extern "C" fn(Slot, Slot),
    merge: unsafe extern "C" fn(Slot, Slot),
    emit: unsafe extern "C" fn(Slot, Slot),
}

unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::group_public_access_conditions"]
    fn group_public_access_conditions(stride: usize, host: AccessGroupHost);
}

struct AccessGrouping<'a> {
    input: std::collections::btree_set::IntoIter<PublicAccess>,
    output: &'a mut BTreeSet<PublicAccess>,
    normal: &'a mut bounds::Normalizer,
    current: Option<AccessGroup>,
}

impl AccessGrouping<'_> {
    unsafe extern "C" fn next(context: Slot) -> Borrowed {
        let state = unsafe { &mut *context.cast::<Self>() };
        let Some(mut access) = state.input.next() else { return std::ptr::null(); };
        let normal = &mut *state.normal;
        match &mut access {
            PublicAccess::Resource { resources, borrows, .. } => {
                resources.normalize_contract(normal);
                borrows.normalize_contract(normal);
            }
            PublicAccess::Position { borrows, position, .. } => {
                borrows.normalize_contract(normal);
                if let Some(value) = &mut position.written { normal.proof(value); }
            }
            PublicAccess::Memory { borrows, .. } | PublicAccess::Dataset { borrows, .. } => {
                borrows.normalize_contract(normal);
            }
            PublicAccess::Initialized { borrows, initialized, .. } => {
                borrows.normalize_contract(normal);
                initialization(initialized, normal);
            }
            PublicAccess::Stored { borrows, value, .. } => {
                borrows.normalize_contract(normal);
                value.normalize_contract(normal);
            }
        }
        let condition = std::mem::replace(access.when_mut(), bounds::Condition::constant(true));
        let entry = state.current.insert(AccessGroup { key: access, condition });
        std::ptr::from_ref(&entry.key).cast()
    }

    unsafe extern "C" fn compare(access: Borrowed, entry: Borrowed) -> i32 {
        unsafe { (&*access.cast::<PublicAccess>()).cmp(&(*entry.cast::<AccessGroup>()).key) as i32 }
    }

    unsafe extern "C" fn seed(context: Slot, entry: Slot) {
        let state = unsafe { &mut *context.cast::<Self>() };
        let mut group = state.current.take().unwrap();
        group.condition = state.normal.condition(&group.condition);
        unsafe { entry.cast::<AccessGroup>().write(group) };
    }

    unsafe extern "C" fn merge(context: Slot, entry: Slot) {
        let state = unsafe { &mut *context.cast::<Self>() };
        let AccessGroup { condition, .. } = state.current.take().unwrap();
        let group = unsafe { &mut *entry.cast::<AccessGroup>() };
        group.condition = state.normal.union(&group.condition, &condition);
    }

    unsafe extern "C" fn emit(context: Slot, entry: Slot) {
        let state = unsafe { &mut *context.cast::<Self>() };
        let AccessGroup { mut key, condition } = unsafe { entry.cast::<AccessGroup>().read() };
        *key.when_mut() = condition;
        state.output.insert(key);
    }
}

fn accesses(values: &mut BTreeSet<PublicAccess>, normal: &mut bounds::Normalizer) {
    const STRIDE: usize = {
        assert!(align_of::<AccessGroup>() <= 16);
        assert!(std::mem::offset_of!(AccessGroupRow, group) == 32);
        size_of::<AccessGroupRow>()
    };
    let mut grouping = AccessGrouping { input: std::mem::take(values).into_iter(), output: values, normal, current: None };
    let host = AccessGroupHost {
        context: std::ptr::from_mut(&mut grouping).cast(), next: AccessGrouping::next,
        compare: AccessGrouping::compare, seed: AccessGrouping::seed, merge: AccessGrouping::merge,
        emit: AccessGrouping::emit,
    };
    unsafe { group_public_access_conditions(STRIDE, host) };
}

pub(super) fn encode(original:&Contract,writer:&mut crate::module::codec::Writer) {
    let mut normal=bounds::Normalizer::new();let mut first=0;
    for parameter in &original.parameters {
        match &parameter.kind {
            CallParameterKind::Value(ty)=>{
                for(component,&(scalar,_))in ty.components.iter().enumerate(){normal.scalar(ValueIdentity::Register(first+component),scalar);}
                if ty.dataset.is_some()||matches!(ty.name.as_str(),"__blink_borrowed_dataset"|"__blink_address_dataset") {
                    normal.dataset(ValueIdentity::Register(first),ValueIdentity::Register(first+1));
                }
                first+=ty.components.len();
            },
            // ABI slots do not reconstruct source parameter numbers (Frame and
            // resource-only parameters need not occupy source argument slots).
            CallParameterKind::Reference{..}=>first+=2,
        }
    }
    for value in original.returned.iter().chain(original.written.iter().flatten()){describe(value,&mut normal);}
    let mut returned=original.returned.clone();
    let mut written=original.written.clone();
    let mut residency=original.residency.clone();
    let mut view_access=original.view_access.clone();
    let mut initialized=original.initialization.clone();
    let mut domain=original.domain.clone();
    for value in returned.iter_mut().chain(written.iter_mut().flatten()){model(value,&mut normal);}
    residency.normalize_contract(&mut normal);
    if let Some(access)=&mut view_access{accesses(access,&mut normal);}
    if let Some(values)=&mut initialized{initialization(values,&mut normal);}
    if let Some(domain)=&mut domain{for proof in domain.elements.values_mut(){normal.proof(proof);}}
    drop(normal);
    unsafe{Contract::write_public(writer,&original.parameters,&original.result,&original.inputs,&original.element_inputs,&returned,&written,&original.requires,&original.target,&original.frame,&original.fallible,&domain,&residency,view_access.as_ref().unwrap(),initialized.as_ref().unwrap());}
}
