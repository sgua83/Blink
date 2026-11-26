//! B projects facts directly from original proof/value fields into the result.
use super::*;
use std::{collections::btree_set,ffi::c_void,ptr};
type Slot=*mut c_void;
type Borrowed=*const c_void;
#[repr(C)]
struct Input {
    scalar:fact_fields::ScalarView,scalar_ref:Borrowed,primary:Borrowed,
    origins:usize,ranges:usize,mode:u32,nonzero:u32,constant:u32,bits:u64,numeric_only:u32,
}
#[repr(C)]
struct Bounds {lower:u64,upper:u64}
#[repr(C)]
struct Domain {fields:index_domains::Fields,known_bits:u32}
struct Context<'a,'b> {
    proof:&'a Proof,value:Option<&'a Value>,component:usize,
    domains:btree_set::Iter<'a,IndexDomain>,facts:&'b mut Facts,
    output: Option<Output<'a>>,
    mapped_domain: IndexDomain,
}
unsafe fn frame<'a,'b,'c>(slot:Slot)->&'a mut Context<'b,'c>{unsafe{&mut *slot.cast()}}
unsafe extern "C" fn origin(slot:Slot,index:usize)->Borrowed {
    let c = unsafe { frame(slot) };
    let identity = &c.proof.origins[c.component][index];
    let mapped = c.output.as_ref().and_then(|output| output.bindings.get(identity));
    ptr::from_ref(mapped.unwrap_or(identity)).cast()
}
unsafe extern "C" fn range(slot:Slot,index:usize)->Bounds {
    let c=unsafe{frame(slot)};let value=c.proof.interval_source(c.component)[index];Bounds{lower:value.lower,upper:value.upper}
}
unsafe extern "C" fn rewind(slot:Slot) {
    let c=unsafe{frame(slot)};c.domains=c.proof.domains.get(c.component).map(|d|d.iter()).unwrap_or_default();
}
unsafe extern "C" fn next(slot:Slot)->Domain {
    let c = unsafe { frame(slot) };
    let Some(domain) = c.domains.next() else {
        return Domain { fields: index_domains::Fields::default(), known_bits: 0 };
    };
    let domain = match c.output.as_ref().filter(|output| !output.bindings.is_empty()) {
        Some(output) => {
            c.mapped_domain = map_domain(*domain, &|id| output_identity(id, output.bindings));
            &c.mapped_domain
        }
        None => domain,
    };
    Domain {
        fields: index_domains::fields(domain),
        known_bits: u32::from(matches!(domain, IndexDomain::KnownBits(..))),
    }
}
unsafe extern "C" fn boolean_cases(slot:Slot)->u32 {
    let c=unsafe{frame(slot)};
    let produced;
    let cases = if let Some(output) = &c.output {
        output.conditions.binary_search_by_key(&c.component, |(index, _)| *index)
            .ok().map(|index| &output.roots[2 * index..2 * index + 2])
    } else if let Some(value) = c.value {
        produced = Condition::component(value, c.component);
        Some(produced.cases.as_slice())
    } else {
        c.proof.component_condition(c.component).map(|condition| condition.cases.as_slice())
    };
    cases.map_or(0, |cases| {
        cases.iter().enumerate().fold(0, |mask, (truth, case)| {
            mask | if matches!(&**case.0, ClauseNode::Impossible) { 1 << truth } else { 0 }
        })
    })
}
unsafe extern "C" fn emit(slot:Slot,fields:fact_fields::Fields) {
    // The B projection can still be borrowing the current mapped domain.
    unsafe { (*slot.cast::<Context<'_, '_>>()).facts.insert(fact_fields::key(fields)) };
}
#[repr(C)]
struct Host {
    origin:unsafe extern "C" fn(Slot,usize)->Borrowed,
    range:unsafe extern "C" fn(Slot,usize)->Bounds,
    rewind:unsafe extern "C" fn(Slot),next:unsafe extern "C" fn(Slot)->Domain,
    boolean_cases:unsafe extern "C" fn(Slot)->u32,
    emit:unsafe extern "C" fn(Slot,fact_fields::Fields),
    compare:unsafe extern "C" fn(Borrowed,Borrowed)->i32,
}
unsafe extern "C" {
    #[link_name="bootstrap.condition_graph::proof_facts"]
    fn project(context:Slot,input:Input,host:Host);
}
const HOST: Host = Host {
    origin, range, rewind, next, boolean_cases, emit, compare: fact_fields::compare,
};

fn fields(proof:&Proof,value:Option<&Value>,component:usize,scalar:&Scalar,primary:Option<&ValueIdentity>,mode:u32,numeric_only:bool) -> Input {
    let scalar_ref=ptr::from_ref(scalar).cast();
    let constant=value.and_then(|v|v.constant.as_ref()).map(|v|v[component]);
    Input{scalar:unsafe{fact_fields::scalar(scalar_ref)},scalar_ref,
        primary:primary.map(|id|ptr::from_ref(id).cast()).unwrap_or(ptr::null()),
        origins:proof.origins.get(component).map_or(0,Vec::len),ranges:proof.ranges.get(component).map_or(1,|r|r.0.len()),
        mode,nonzero:u32::from(proof.nonzero.get(component)==Some(&true)),constant:u32::from(constant.is_some()),bits:constant.unwrap_or(0),numeric_only:u32::from(numeric_only)}
}
fn append(facts:&mut Facts,proof:&Proof,value:Option<&Value>,component:usize,scalar:&Scalar,primary:Option<&ValueIdentity>,mode:u32,numeric_only:bool) {
    let input = fields(proof, value, component, scalar, primary, mode, numeric_only);
    let mut context=Context{proof,value,component,domains:btree_set::Iter::default(),facts,output:None,mapped_domain:IndexDomain::Tile};
    unsafe{project((&mut context as *mut Context<'_,'_>).cast(),input,HOST)};
}
pub(super) fn collect(proof:&Proof,ty:&Ty)->Facts {
    let mut facts=Facts::new();
    append_proof(&mut facts,proof,ty);
    facts
}
pub(super) fn append_proof(facts:&mut Facts,proof:&Proof,ty:&Ty) {
    for (component,(scalar,_)) in ty.components.iter().enumerate(){append(facts,proof,None,component,scalar,None,0,false);}
}
pub(super) fn append_value(facts:&mut Facts,value:&Value,numeric_only:bool) {
    for component in 0..value.regs.len(){
        let fallback=ValueIdentity::Register(value.regs[component]);
        append(facts,&value.proof,Some(value),component,&value.ty.components[component].0,Some(&fallback),1,numeric_only);
    }
}
pub(super) fn value(value:&Value)->Facts {
    let mut facts=Facts::new();append_value(&mut facts,value,false);facts
}
pub(super) fn append_identity(facts:&mut Facts,proof:&Proof,scalar:Scalar,identity:ValueIdentity) {
    append(facts,proof,None,0,&scalar,Some(&identity),2,false);
}
pub(super) fn append_component(facts:&mut Facts,proof:&Proof,component:usize,scalar:Scalar) {
    append(facts,proof,None,component,&scalar,None,0,false);
}

struct Output<'a> {
    registers: &'a [Reg],
    types: &'a [Scalar],
    bindings: &'a OutputBindings,
    conditions: Vec<(usize, ValueIdentity)>,
    roots: Vec<Clause>,
    aliases: Clause,
}

unsafe extern "C" fn output_input(slot: Slot, component: usize, primary: *mut ValueIdentity) -> Input {
    let context = unsafe { frame(slot) };
    context.component = component;
    let output = unsafe { context.output.as_ref().unwrap_unchecked() };
    let reg = output.registers[component];
    // B borrows this Rust value only for the current component.
    let primary = unsafe {
        primary.write(ValueIdentity::Register(reg));
        &*primary
    };
    fields(context.proof, None, component, &output.types[reg], Some(primary), 3, false)
}

unsafe extern "C" fn finish_values(slot: Slot) {
    let context = unsafe { frame(slot) };
    let output = unsafe { context.output.as_mut().unwrap_unchecked() };
    output.aliases = Clause::facts(std::mem::take(context.facts));
}

unsafe extern "C" fn case_identity(slot: Slot, index: usize) -> Borrowed {
    let output = unsafe { (&*slot.cast::<Context<'_, '_>>()).output.as_ref().unwrap_unchecked() };
    ptr::from_ref(&output.conditions[index].1).cast()
}

unsafe extern "C" fn join_cases(slot: Slot, index: usize, no: fact_fields::Fields, yes: fact_fields::Fields) {
    let (no, yes) = unsafe { (fact_fields::key(no), fact_fields::key(yes)) };
    let output = unsafe { frame(slot).output.as_mut().unwrap_unchecked() };
    let no = Clause::both(&output.roots[2 * index], &Clause::facts([no].into()));
    let yes = Clause::both(&output.roots[2 * index + 1], &Clause::facts([yes].into()));
    output.aliases = Clause::both(&output.aliases, &Clause::either(&no, &yes));
}

#[repr(C)]
struct OutputHost {
    input: unsafe extern "C" fn(Slot, usize, *mut ValueIdentity) -> Input,
    finish_values: unsafe extern "C" fn(Slot),
    case_identity: unsafe extern "C" fn(Slot, usize) -> Borrowed,
    join_cases: unsafe extern "C" fn(Slot, usize, fact_fields::Fields, fact_fields::Fields),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::output_aliases"]
    fn project_output(context: Slot, components: usize, conditions: usize, primary: *mut ValueIdentity, host: OutputHost, facts: Host);
}

pub(super) fn output_aliases(proof: &Proof, registers: &[Reg], types: &[Scalar], bindings: &OutputBindings) -> Clause {
    let mut conditions = Vec::new();
    let mut roots = Vec::new();
    for (component, &reg) in registers.iter().enumerate() {
        if types[reg] == Scalar::Bool && let Some(condition) = proof.component_condition(component) {
            conditions.push((component, ValueIdentity::Register(reg)));
            roots.extend_from_slice(&condition.cases);
        }
    }
    let roots = Clause::output_bounds(&roots, bindings);
    let count = conditions.len();
    let output = Output {
        registers, types, bindings, conditions, roots,
        aliases: Clause::default(),
    };
    let mut values = Facts::new();
    let mut primary = std::mem::MaybeUninit::uninit();
    let mut context = Context {
        proof, value: None, component: 0, domains: btree_set::Iter::default(),
        facts: &mut values, output: Some(output), mapped_domain: IndexDomain::Tile,
    };
    unsafe {
        project_output(
            (&mut context as *mut Context<'_, '_>).cast(), registers.len(), count, primary.as_mut_ptr(),
            OutputHost { input: output_input, finish_values, case_identity, join_cases }, HOST,
        );
    }
    context.output.unwrap().aliases
}
