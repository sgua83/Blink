//! B owns the alias fact table and reads it during the original contradiction check.
use super::{Fact, Facts, Scalar, ValueIdentity, fact_fields, facts};
use std::{collections::BTreeMap, ffi::c_void, ptr};

type Borrowed = *const c_void;
type Roots = BTreeMap<ValueIdentity, ValueIdentity>;
type Lookup = unsafe extern "C" fn(Borrowed, &ValueIdentity) -> *const ValueIdentity;

#[derive(Default)]
#[repr(C)]
struct AliasFacts {
    records: *mut c_void,
    root: usize,
    count: usize,
    capacity: usize,
}

#[repr(C)]
struct Scalars {
    values: [Borrowed; 4],
}

unsafe extern "C" {
    #[link_name = "bootstrap.proof_facts::alias_facts_build"]
    fn build_aliases(result: &mut AliasFacts, source: &Facts, roots: Borrowed, lookup: Lookup);
    #[link_name = "bootstrap.proof_facts::alias_facts_insert"]
    fn insert_alias(table: &mut AliasFacts, fact: &Fact, roots: Borrowed, lookup: Lookup) -> u32;
    #[link_name = "bootstrap.proof_facts::alias_facts_remove"]
    fn remove_alias(table: &mut AliasFacts, fact: &Fact, roots: Borrowed, lookup: Lookup);
    #[link_name = "bootstrap.condition_graph::internal_conflict"]
    fn evaluate(owner: Borrowed, aliases: u32, integers: Scalars,
        predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView) -> u32;
}

unsafe extern "C" fn lookup(roots: Borrowed, identity: &ValueIdentity) -> *const ValueIdentity {
    unsafe { &*roots.cast::<Roots>() }.get(identity).map_or(ptr::null(), ptr::from_ref)
}

impl AliasFacts {
    fn new(facts: &Facts, roots: &Roots) -> Self {
        let mut result = Self::default();
        unsafe { build_aliases(&mut result, facts, ptr::from_ref(roots).cast(), lookup) };
        result
    }
}

impl Drop for AliasFacts {
    fn drop(&mut self) {
        unsafe { facts::clear_tree(ptr::from_mut(self).cast()) };
    }
}

struct Aliases {
    roots: Roots,
    facts: AliasFacts,
}

pub(super) struct Prepared {
    aliases: Option<Aliases>,
}

impl Prepared {
    pub(super) fn new(facts: &Facts) -> Self {
        let roots = super::equality_roots(facts);
        let aliases = (!roots.is_empty()).then(|| Aliases { facts: AliasFacts::new(facts, &roots), roots });
        Self { aliases }
    }

    pub(super) fn insert(&mut self, fact: Fact) -> bool {
        self.aliases.as_mut().is_none_or(|aliases|
            unsafe { insert_alias(&mut aliases.facts, &fact, ptr::from_ref(&aliases.roots).cast(), lookup) != 0 })
    }

    pub(super) fn remove(&mut self, fact: Fact) {
        if let Some(aliases) = &mut self.aliases {
            unsafe { remove_alias(&mut aliases.facts, &fact, ptr::from_ref(&aliases.roots).cast(), lookup) };
        }
    }

    pub(super) fn conflict(&self, facts: &Facts) -> bool {
        match &self.aliases {
            Some(aliases) => check(ptr::from_ref(&aliases.facts).cast(), true),
            None => check(ptr::from_ref(facts).cast(), false),
        }
    }
}

pub(super) fn conflict(facts: &Facts, roots: &Roots) -> bool {
    if roots.is_empty() {
        check(ptr::from_ref(facts).cast(), false)
    } else {
        let aliases = AliasFacts::new(facts, roots);
        check(ptr::from_ref(&aliases).cast(), true)
    }
}

fn check(owner: Borrowed, aliases: bool) -> bool {
    let integers = [Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64];
    let scalars = Scalars { values: integers.each_ref().map(|value| ptr::from_ref(value).cast()) };
    unsafe { evaluate(owner, u32::from(aliases), scalars, fact_fields::predicate) != 0 }
}
