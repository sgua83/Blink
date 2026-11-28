use super::{Clause, ClauseIdentity, Facts, RequiredFacts};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
};

struct Root {
    // A weak owner keeps the allocation identity valid for this function.
    _source: ClauseIdentity,
    environments: BTreeMap<Facts, Option<RequiredFacts>>,
}

type Results = HashMap<usize, Root>;

thread_local! {
    static RESULTS: RefCell<Option<Results>> = const { RefCell::new(None) };
}

pub(super) fn scope() -> impl Drop {
    struct Scope(Option<Results>);
    impl Drop for Scope {
        fn drop(&mut self) {
            RESULTS.replace(self.0.take());
        }
    }
    Scope(RESULTS.replace(Some(Results::new())))
}

fn result<R>(
    root: &Clause,
    environment: &Facts,
    read: impl FnOnce(&Option<RequiredFacts>) -> R,
) -> Option<R> {
    RESULTS.with_borrow(|active| {
        let known = active.as_ref()?.get(&root.key())?;
        known.environments.get(environment).map(read)
    })
}

fn remember(
    root: &Clause,
    environment: &Facts,
    value: impl FnOnce() -> Option<RequiredFacts>,
) {
    RESULTS.with_borrow_mut(|active| {
        if let Some(results) = active {
            results.entry(root.key()).or_insert_with(|| Root {
                _source: root.retain_identity(),
                environments: BTreeMap::new(),
            }).environments.insert(environment.clone(), value());
        }
    });
}

pub(super) fn possible(
    root: &Clause,
    environment: &Facts,
    evaluate: impl FnOnce() -> Option<RequiredFacts>,
) -> bool {
    if let Some(known) = result(root, environment, Option::is_some) {
        return known;
    }
    let value = evaluate();
    let possible = value.is_some();
    remember(root, environment, || value);
    possible
}

pub(super) fn required(
    root: &Clause,
    environment: &Facts,
    evaluate: impl FnOnce() -> Option<RequiredFacts>,
) -> Option<RequiredFacts> {
    if let Some(known) = result(root, environment, Clone::clone) {
        return known;
    }
    let value = evaluate();
    remember(root, environment, || value.clone());
    value
}
