//! The native fold borrows the original environment, facts and equality roots.
use super::{
    EqualityRoots, Facts, QueryFacts, additional_equality_roots,
    equality::{self, Members},
};
use std::{borrow::Cow, cell::OnceCell, sync::Arc};

#[repr(C)]
struct Inputs<'a> {
    environment: &'a Facts,
    sides: [&'a Facts; 2],
    roots: [&'a EqualityRoots; 2],
}

unsafe extern "C" {
    #[link_name = "bootstrap.common_facts::common_facts"]
    fn evaluate(input: Inputs<'_>, output: &mut Facts);
}

fn fold<'a>(environment: &'a Facts, sides: [&'a Facts; 2], roots: [&'a EqualityRoots; 2]) -> Facts {
    let mut result = Facts::new();
    unsafe { evaluate(Inputs { environment, sides, roots }, &mut result) };
    result
}

pub(super) fn either<'a>(
    environment: &Facts,
    environment_members: &Members<'_>,
    shared_equalities: Option<&OnceCell<Arc<Facts>>>,
    a: Option<QueryFacts<'a>>,
    b: Option<QueryFacts<'a>>,
) -> Option<QueryFacts<'a>> {
    match (a, b) {
        // With no established facts, an empty side supplies no equality,
        // range or known bits to the disjunction. Reuse its empty result.
        (Some(a), Some(_)) if environment.is_empty() && a.is_empty() => Some(a),
        (Some(_), Some(b)) if environment.is_empty() && b.is_empty() => Some(b),
        (Some(a), Some(b)) => {
            let environment_roots = environment_members.roots();
            let left = additional_equality_roots(environment, environment_roots, &a);
            let right = additional_equality_roots(environment, environment_roots, &b);
            let expand =
                |facts, roots: &Cow<'_, EqualityRoots>| match roots {
                    Cow::Borrowed(_) => {
                        equality::equivalent(environment, facts, environment_members, None)
                    }
                    Cow::Owned(_) => equality::equivalent(
                        environment, facts, &Members::new(roots), Some(environment_roots),
                    ),
                };
            let a = expand(a, &left);
            let b = expand(b, &right);
            let common = if let Some(original) = shared_equalities
                && matches!(left, Cow::Borrowed(_))
                && matches!(right, Cow::Borrowed(_))
            {
                // Empty sides leave only the original equality result to produce.
                let shared = original.get_or_init(|| {
                    let empty = Facts::new();
                    Arc::new(fold(environment, [&empty, &empty], [environment_roots, environment_roots]))
                });
                let empty_roots = EqualityRoots::new();
                let mut facts = fold(environment, [&a, &b], [&empty_roots, &empty_roots]);
                if facts.is_empty() {
                    return Some(QueryFacts::Owned(Arc::clone(shared)));
                }
                facts.extend(shared.iter().copied());
                facts
            } else {
                fold(environment, [&a, &b], [&left, &right])
            };
            Some(QueryFacts::Owned(Arc::new(common)))
        }
        (a, None) | (None, a) => a,
    }
}
