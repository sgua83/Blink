//! The sole public contract projection. Checking keeps its original shared
//! facts; this temporary value is consumed by the original encoder.
use super::*;
use crate::module::codec::Encode;

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
    for value in values{normal.origin(&mut value.origin);value.when=normal.condition(&value.when);}
}
fn accesses(values:&mut BTreeSet<view_contracts::Access>,normal:&mut bounds::Normalizer) {
    use view_contracts::Access;
    let mut grouped:BTreeMap<Access,bounds::Condition>=BTreeMap::new();
    for mut access in std::mem::take(values) {
        let when=match &mut access {
            Access::Resource{resources,borrows,when,..}=>{resources.normalize_contract(normal);borrows.normalize_contract(normal);when},
            Access::Position{borrows,when,position}=>{borrows.normalize_contract(normal);if let Some(value)=&mut position.written{normal.proof(value);}when},
            Access::Memory{borrows,when,..}|Access::Dataset{borrows,when,..}=>{borrows.normalize_contract(normal);when},
            Access::Initialized{borrows,initialized,when}=>{borrows.normalize_contract(normal);initialization(initialized,normal);when},
            Access::Stored{borrows,value,when,..}=>{borrows.normalize_contract(normal);value.normalize_contract(normal);when},
        };
        let when=std::mem::replace(when,bounds::Condition::constant(true));
        grouped.entry(access).and_modify(|old|*old=normal.union(old,&when)).or_insert_with(||normal.condition(&when));
    }
    for(mut access,condition)in grouped {
        let when=match &mut access{Access::Resource{when,..}|Access::Position{when,..}|Access::Memory{when,..}|Access::Dataset{when,..}|Access::Initialized{when,..}|Access::Stored{when,..}=>when};
        *when=condition;values.insert(access);
    }
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
    let mut public=original.clone();
    for value in public.returned.iter_mut().chain(public.written.iter_mut().flatten()){model(value,&mut normal);}
    public.residency.normalize_contract(&mut normal);
    if let Some(access)=&mut public.view_access{accesses(access,&mut normal);}
    if let Some(values)=&mut public.initialization{initialization(values,&mut normal);}
    if let Some(domain)=&mut public.domain{for proof in domain.elements.values_mut(){normal.proof(proof);}}
    public.parameters.encode(writer);public.result.encode(writer);public.inputs.encode(writer);public.element_inputs.encode(writer);
    public.returned.encode(writer);public.written.encode(writer);public.requires.encode(writer);
    public.target.encode(writer);public.frame.encode(writer);public.fallible.encode(writer);public.domain.encode(writer);
    public.residency.encode(writer);public.view_access.as_ref().unwrap().encode(writer);public.initialization.as_ref().unwrap().encode(writer);
}
