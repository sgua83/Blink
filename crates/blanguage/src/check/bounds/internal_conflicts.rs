//! B decides internal contradictions; the original typed containers own facts and aliases.
use super::facts;
use super::fact_fields::{ScalarView, same_scalar, scalar};
use super::{Fact, Facts, Scalar, ValueIdentity, fact_fields, map_domain};
use std::{
    collections::{BTreeMap, btree_map},
    ffi::c_void,
    ptr,
};

type Borrowed = *const c_void;
type Slot = *mut c_void;

type Roots = BTreeMap<ValueIdentity, ValueIdentity>;
type OrderedFacts = BTreeMap<Fact, usize>;

fn canonical(fact: Fact, roots: &Roots) -> Option<Fact> {
    let root = |id| roots.get(&id).copied().unwrap_or(id);
    Some(match fact {
        Fact::Equal(..) => return None,
        Fact::NotEqual(a, b) => {
            let (a, b) = (root(a), root(b));
            Fact::NotEqual(a.min(b), a.max(b))
        }
        Fact::Below(a, b, strict, scalar) => Fact::Below(root(a), root(b), strict, scalar),
        Fact::Predicate(predicate, truth) => {
            Fact::Predicate(predicate.map(|id| Some(root(id))).unwrap(), truth)
        }
        Fact::Index(id, domain) => Fact::Index(root(id), map_domain(domain, &root)),
        Fact::Outside(id, domain) => Fact::Outside(root(id), map_domain(domain, &root)),
        value => value.identity().map(|id| value.at(root(id))).unwrap_or(value),
    })
}

fn ordered_facts(facts: &Facts, roots: &Roots) -> OrderedFacts {
    let mut ordered = OrderedFacts::new();
    for fact in facts.iter().copied().filter_map(|fact| canonical(fact, roots)) {
        *ordered.entry(fact).or_default() += 1;
    }
    ordered
}

struct Aliases {
    roots: Roots,
    facts: OrderedFacts,
}

pub(super) struct Prepared {
    aliases: Option<Aliases>,
}

impl Prepared {
    pub(super) fn new(facts: &Facts) -> Self {
        let roots = super::equality_roots(facts);
        let aliases = if roots.is_empty() {
            None
        } else {
            Some(Aliases { facts: ordered_facts(facts, &roots), roots })
        };
        Self { aliases }
    }

    pub(super) fn insert(&mut self, fact: Fact) -> bool {
        let Some(aliases) = &mut self.aliases else { return true; };
        if let Some(key) = canonical(fact, &aliases.roots)
            && let btree_map::Entry::Vacant(entry) = aliases.facts.entry(key)
        {
            entry.insert(1);
            return true;
        }
        false
    }

    pub(super) fn remove(&mut self, fact: Fact) {
        if let Some(aliases) = &mut self.aliases
            && let Some(key) = canonical(fact, &aliases.roots)
            && let btree_map::Entry::Occupied(mut entry) = aliases.facts.entry(key)
        {
            if *entry.get() == 1 {
                entry.remove();
            } else {
                *entry.get_mut() -= 1;
            }
        }
    }

    pub(super) fn conflict(&self, facts: &Facts) -> bool {
        match &self.aliases {
            Some(aliases) => check(Input::aliased(&aliases.facts)),
            None => check(Input::direct(facts)),
        }
    }
}

enum Input<'a> {
    Direct {
        facts: &'a Facts,
        cursor: facts::Iter<'a>,
    },
    Aliased {
        facts: &'a OrderedFacts,
        cursor: btree_map::Keys<'a, Fact, usize>,
    },
}
impl<'a> Input<'a> {
    fn direct(facts: &'a Facts) -> Self {
        Self::Direct { facts, cursor: facts.iter() }
    }
    fn aliased(facts: &'a OrderedFacts) -> Self {
        Self::Aliased { facts, cursor: facts.keys() }
    }
    fn select(&self, query: Fact, preceding: bool) -> Option<&Fact> {
        match self {
            Self::Direct { facts, .. } => {
                if preceding {
                    facts.range(..=query).next_back()
                } else {
                    facts.range(query..).next()
                }
            }
            Self::Aliased { facts, .. } => {
                let entry = if preceding {
                    facts.range(..=query).next_back()
                } else {
                    facts.range(query..).next()
                };
                entry.map(|(fact, _)| fact)
            }
        }
    }
}
unsafe fn input<'a, 'b>(slot: Slot) -> &'a mut Input<'b> {
    unsafe { &mut *slot.cast::<Input<'b>>() }
}
unsafe extern "C" fn next(slot: Slot) -> fact_fields::Fields {
    let fact = match unsafe { input(slot) } {
        Input::Direct { cursor, .. } => cursor.next(),
        Input::Aliased { cursor, .. } => cursor.next(),
    };
    fact.map(fact_fields::Fields::from).unwrap_or_default()
}
unsafe extern "C" fn select(
    slot: Slot,
    key: fact_fields::Fields,
    preceding: u32,
) -> fact_fields::Fields {
    unsafe { input(slot) }
        .select(unsafe { fact_fields::key(key) }, preceding != 0)
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn contains(slot: Slot, fields: fact_fields::Fields) -> u32 {
    let key = unsafe { fact_fields::key(fields) };
    u32::from(
        unsafe { input(slot) }
            .select(key, false)
            .is_some_and(|fact| *fact == key),
    )
}
#[repr(C)]
struct Scalars {
    values: [Borrowed; 4],
}
#[repr(C)]
struct Host {
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    select: unsafe extern "C" fn(Slot, fact_fields::Fields, u32) -> fact_fields::Fields,
    contains: unsafe extern "C" fn(Slot, fact_fields::Fields) -> u32,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    same_scalar: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    scalar: unsafe extern "C" fn(Borrowed) -> ScalarView,
    predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView,
    upper: unsafe extern "C" fn(Borrowed) -> fact_fields::UpperView,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::internal_conflict"]
    fn evaluate(context: Slot, integers: Scalars, host: Host) -> u32;
}
pub(super) fn conflict(facts: &Facts, roots: &Roots) -> bool {
    if roots.is_empty() {
        check(Input::direct(facts))
    } else {
        let facts = ordered_facts(facts, roots);
        check(Input::aliased(&facts))
    }
}

fn check(mut input: Input<'_>) -> bool {
    let integers = [Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64];
    let scalars = Scalars {
        values: integers.each_ref().map(|value| ptr::from_ref(value).cast()),
    };
    unsafe {
        evaluate(
            (&mut input as *mut Input<'_>).cast(),
            scalars,
            Host {
                next,
                select,
                contains,
                compare: fact_fields::compare,
                same_scalar,
                scalar,
                predicate: fact_fields::predicate,
                upper: fact_fields::upper,
            },
        ) != 0
    }
}
