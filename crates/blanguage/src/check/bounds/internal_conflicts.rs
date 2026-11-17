//! B decides internal contradictions; the original typed containers own facts and aliases.
use super::fact_fields::{ScalarView, same_scalar, scalar};
use super::{Fact, Facts, Scalar, ValueIdentity, fact_fields, map_domain};
use std::{
    collections::{BTreeMap, btree_set},
    ffi::c_void,
    ptr,
};

type Borrowed = *const c_void;
type Slot = *mut c_void;

enum Input<'a> {
    Direct {
        facts: &'a Facts,
        cursor: btree_set::Iter<'a, Fact>,
    },
    Aliased {
        facts: Vec<Fact>,
        cursor: usize,
    },
}
impl<'a> Input<'a> {
    fn new(facts: &'a Facts, roots: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        if roots.is_empty() {
            return Self::Direct {
                facts,
                cursor: facts.iter(),
            };
        }
        let root = |id| roots.get(&id).copied().unwrap_or(id);
        let canonical = |fact: Fact| match fact {
            Fact::Equal(a, b) => Fact::equal(root(a), root(b)),
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
            value => value
                .identity()
                .map(|id| value.at(root(id)))
                .unwrap_or(value),
        };
        let mut values: Vec<_> = facts.iter().copied().map(canonical).collect();
        values.sort_unstable();
        values.dedup();
        Self::Aliased {
            facts: values,
            cursor: 0,
        }
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
                let index = facts.partition_point(|fact| {
                    if preceding {
                        *fact <= query
                    } else {
                        *fact < query
                    }
                });
                if preceding {
                    index.checked_sub(1).and_then(|index| facts.get(index))
                } else {
                    facts.get(index)
                }
            }
        }
    }
}
unsafe fn input<'a, 'b>(slot: Slot) -> &'a mut Input<'b> {
    unsafe { &mut *slot.cast::<Input<'b>>() }
}
unsafe extern "C" fn next(slot: Slot) -> fact_fields::Fields {
    match unsafe { input(slot) } {
        Input::Direct { cursor, .. } => cursor
            .next()
            .map(fact_fields::Fields::from)
            .unwrap_or_default(),
        Input::Aliased { facts, cursor } => {
            let value = facts.get(*cursor);
            *cursor += usize::from(value.is_some());
            value.map(fact_fields::Fields::from).unwrap_or_default()
        }
    }
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
pub(super) fn conflict(facts: &Facts, roots: &BTreeMap<ValueIdentity, ValueIdentity>) -> bool {
    let mut input = Input::new(facts, roots);
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
