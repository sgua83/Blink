//! 独占位置保留原存储：元素函数静态投影，CPU 普通调用传递实际借用地址。
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ApplicationIndex {
    Constant(u32),
    Value(ValueIdentity),
}
impl ApplicationIndex {
    pub(super) fn of(value: &Value) -> Self {
        if value.minimum() == value.maximum() {
            Self::Constant(value.minimum())
        } else {
            Self::Value(value.identity())
        }
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Position {
    Local(usize),
    Element(usize, u32),
    Parameter(usize, usize),
    ApplicationElement(usize, ApplicationIndex, u32),
    Indexed(std::sync::Arc<[Position]>, ApplicationIndex),
}
impl Position {
    pub(super) fn initialized_by(&self, initialized: &BTreeSet<Position>) -> bool {
        initialized.contains(self)
            || match self {
                Self::Indexed(choices, _) => choices
                    .iter()
                    .all(|choice| choice.initialized_by(initialized)),
                Self::ApplicationElement(scope, _, word) => {
                    initialized.contains(&Self::Parameter(*scope, *word as usize))
                }
                _ => false,
            }
    }
    pub(super) fn uses_scope(&self, scope: usize) -> bool {
        match self {
            Self::Parameter(owner, _) | Self::ApplicationElement(owner, _, _) => *owner == scope,
            Self::Indexed(choices, _) => choices.iter().any(|choice| choice.uses_scope(scope)),
            _ => false,
        }
    }
    pub(super) fn uses_index(&self, index: ApplicationIndex) -> bool {
        if matches!(index, ApplicationIndex::Constant(_)) {
            return false;
        }
        match self {
            Self::ApplicationElement(_, value, _) => *value == index,
            Self::Indexed(choices, value) => {
                *value == index || choices.iter().any(|choice| choice.uses_index(index))
            }
            _ => false,
        }
    }
}
pub(super) fn intersect_initialized(
    a: &BTreeSet<Position>,
    b: &BTreeSet<Position>,
) -> BTreeSet<Position> {
    a.union(b)
        .filter(|position| position.initialized_by(a) && position.initialized_by(b))
        .cloned()
        .collect()
}
impl Place {
    pub(super) fn owned_positions(&self) -> Option<Vec<Position>> {
        match self {
            Self::ApplicationElement {
                initialization,
                words,
                ..
            } => Some(
                initialization
                    .iter()
                    .flat_map(|(scope, index)| {
                        words
                            .iter()
                            .map(|word| Position::ApplicationElement(*scope, *index, *word))
                    })
                    .collect(),
            ),
            Self::Reference { scope, words, .. } => Some(
                words
                    .iter()
                    .map(|word| Position::Parameter(*scope, *word as usize))
                    .collect(),
            ),
            Self::Indexed { initialization, .. } => Some(initialization.clone()),
            Self::ReadOnlyLocal(_, slots, ..) | Self::Local(_, slots, ..) => {
                Some(slots.iter().copied().map(Position::Local).collect())
            }
            Self::Element { binding, words, .. } => Some(
                words
                    .iter()
                    .map(|word| Position::Element(*binding, *word))
                    .collect(),
            ),
            _ => None,
        }
    }
    pub(super) fn ty(&self) -> &Ty {
        match self {
            Self::Reference { ty, .. }
            | Self::AtomicParameter(ty)
            | Self::Local(ty, _, ..)
            | Self::ReadOnlyLocal(ty, _, ..)
            | Self::Indexed { ty, .. }
            | Self::Element { ty, .. }
            | Self::TileElement { ty, .. }
            | Self::ViewElement { ty, .. } => ty,
            Self::ApplicationElement { ty, .. } => ty,
        }
    }
    pub(super) fn components_in(&self, owner: &Place) -> Option<Vec<usize>> {
        match (self, owner) {
            (
                Self::Reference {
                    address: a, words, ..
                },
                Self::Reference {
                    address: b,
                    words: root,
                    ..
                },
            ) if a == b => words
                .iter()
                .map(|word| root.iter().position(|root| root == word))
                .collect(),
            (
                Self::ApplicationElement {
                    identity: a,
                    initialization: ai,
                    index: i,
                    words,
                    ..
                },
                Self::ApplicationElement {
                    identity: b,
                    initialization: bi,
                    index: j,
                    words: root,
                    ..
                },
            ) if a == b && (ai.is_some() && ai == bi || i == j) => words
                .iter()
                .map(|word| root.iter().position(|root| root == word))
                .collect(),
            (
                Self::Indexed {
                    base: a,
                    offset: i,
                    words,
                    ..
                },
                Self::Indexed {
                    base: b,
                    offset: j,
                    words: root,
                    ..
                },
            ) if a == b && i == j => words
                .iter()
                .map(|w| root.iter().position(|r| r == w))
                .collect(),
            (Self::ReadOnlyLocal(_, slots, ..), Self::ReadOnlyLocal(_, root, ..))
            | (Self::Local(_, slots, ..), Self::Local(_, root, ..)) => slots
                .iter()
                .map(|s| root.iter().position(|r| r == s))
                .collect(),
            (
                Self::Element {
                    binding,
                    index,
                    words,
                    ..
                },
                Self::Element {
                    binding: root_binding,
                    index: root_index,
                    words: root,
                    ..
                },
            ) if binding == root_binding && index == root_index => words
                .iter()
                .map(|s| root.iter().position(|r| r == s))
                .collect(),
            _ => None,
        }
    }
    pub(super) fn overlaps(&self, other: &Place) -> bool {
        match (self, other) {
            (
                Self::Reference {
                    address: a, words, ..
                },
                Self::Reference {
                    address: b,
                    words: other,
                    ..
                },
            ) => a == b && words.iter().any(|word| other.contains(word)),
            (
                Self::ApplicationElement {
                    sources: a,
                    index_range: (low_a, high_a),
                    words,
                    ..
                },
                Self::ApplicationElement {
                    sources: b,
                    index_range: (low_b, high_b),
                    words: other,
                    ..
                },
            ) => {
                a.iter().any(|source| b.contains(source))
                    && high_a >= low_b
                    && high_b >= low_a
                    && words.iter().any(|word| other.contains(word))
            }
            (Self::Indexed { base, .. }, b) => base.overlaps(b.storage()),
            (a, Self::Indexed { base, .. }) => a.storage().overlaps(base),
            (Self::Local(_, a, ..), Self::Local(_, b, ..)) => a.iter().any(|x| b.contains(x)),
            (
                Self::Element {
                    binding: a,
                    index: i,
                    words,
                    ..
                },
                Self::Element {
                    binding: b,
                    index: j,
                    words: other,
                    ..
                },
            ) => a == b && i == j && words.iter().any(|x| other.contains(x)),
            _ => false,
        }
    }
}
impl Builder<'_> {
    pub(super) fn mutable_place(&mut self, expr: &Expr, access: Access) -> Result<Option<Place>> {
        let Some(place) = self.place(expr)? else {
            return Ok(None);
        };
        let positions = self.writable_storage(&place, expr.span)?;
        if positions.iter().copied().collect::<BTreeSet<_>>().len() != positions.len() {
            return self.fail(expr.span, "B227", "独占借用位置不能包含重复分量");
        }
        if access == Access::InOut {
            self.require_initialized(&place, expr.span)?;
        }
        Ok(Some(place))
    }
    pub(super) fn writable_storage(&self, place: &Place, span: Span) -> Result<Vec<usize>> {
        if let Place::Indexed { base, words, .. } = place {
            self.writable_storage(base, span)?;
            return Ok(words.iter().map(|w| *w as usize).collect());
        }
        match place {
            Place::Reference { words, .. } => Ok(words.iter().map(|word| *word as usize).collect()),
            Place::ApplicationElement {
                words,
                writable: true,
                ..
            } => Ok(words.iter().map(|word| *word as usize).collect()),
            Place::Local(_, slots, ..) => Ok(slots.clone()),
            Place::Element {
                binding,
                words,
                writable: true,
                ..
            } if matches!(self.view_access[binding], Access::Write | Access::InOut) => {
                Ok(words.iter().map(|w| *w as usize).collect())
            }
            _ => self.fail(
                span,
                "B227",
                "write／inout 需要当前独占元素或可变局部位置，不能借用只读或原子共享位置",
            ),
        }
    }
    pub(super) fn positions(&self, place: &Place) -> Vec<Position> {
        if let Some(positions) = self.env.values().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            let identities = owner.initialization.as_ref()?;
            let components = place.components_in(&owner.place)?;
            Some(components.iter().map(|i| identities[*i].clone()).collect())
        }) {
            return positions;
        }
        place.owned_positions().unwrap()
    }
    pub(super) fn indexed_positions(
        &self,
        place: &Place,
        index: &Value,
        width: usize,
    ) -> Vec<Position> {
        let positions = match place.owned_positions() {
            Some(_) => self.positions(place),
            None => return Vec::new(),
        };
        if positions.is_empty() {
            return positions;
        }
        let index = ApplicationIndex::of(index);
        (0..width)
            .map(|component| {
                let choices: std::sync::Arc<[Position]> = positions
                    .chunks_exact(width)
                    .map(|element| element[component].clone())
                    .collect();
                match index {
                    ApplicationIndex::Constant(index) => choices[index as usize].clone(),
                    _ => Position::Indexed(choices, index),
                }
            })
            .collect()
    }
    pub(super) fn position_initialized_in(
        &self,
        position: &Position,
        initialized: &BTreeSet<Position>,
        environment: &BTreeMap<String, Binding>,
    ) -> bool {
        position.initialized_by(initialized) || match position {
            Position::Indexed(choices, _) => choices.iter().all(|choice| self.position_initialized_in(choice, initialized, environment)),
            Position::ApplicationElement(scope, _, word) => environment.values().any(|binding| matches!(binding,
                Binding::ApplicationData(data) if data.initialization == Some(*scope) && self.application_field_initialized_in(data, *word, initialized))),
            _ => false,
        }
    }
    pub(super) fn is_initialized(&self, place: &Place) -> bool {
        self.positions(place)
            .iter()
            .all(|position| self.position_initialized_in(position, &self.initialized, &self.env))
    }
    pub(super) fn incomplete_arrays(&self) -> bool {
        self.env.values().any(|binding| match binding {
            Binding::Owned(owner) => {
                owner.place.ty().contains_arrays() && !self.is_initialized(&owner.place)
            }
            Binding::OwnedValue(owner) => {
                owner.value.ty.contains_arrays()
                    && owner
                        .place()
                        .is_some_and(|place| !self.is_initialized(&place))
            }
            _ => false,
        }) || self.view_access.iter().any(|(binding, access)| {
            *access == Access::Write
                && self.program.buffers[*binding].ty.contains_arrays()
                && self.program.buffers[*binding]
                    .ty
                    .components
                    .iter()
                    .any(|(_, offset)| {
                        !self
                            .initialized
                            .contains(&Position::Element(*binding, offset / 4))
                    })
        })
    }
    pub(super) fn require_initialized(&self, place: &Place, span: Span) -> Result<()> {
        if !self.is_initialized(place) {
            return self.fail(
                span,
                "B142",
                "读取或 inout 借用了尚未初始化的位置；先完整写入所需分量",
            );
        }
        Ok(())
    }
    pub(super) fn initialize(&mut self, place: &Place) {
        self.initialized.extend(self.positions(place));
    }
    pub(super) fn join_initialization(
        &mut self,
        condition: &Value,
        yes: BTreeSet<Position>,
        no: BTreeSet<Position>,
    ) {
        self.initialized = if let Some(bits) = &condition.constant {
            if bits[0] != 0 { yes } else { no }
        } else {
            intersect_initialized(&yes, &no)
        };
    }
    pub(super) fn borrow_owned(&self, place: Place) -> OwnedBinding {
        for binding in self.env.values() {
            if let Binding::OwnedValue(owner) = binding
                && let Some(root) = owner.place()
                && let Some(components) = place.components_in(&root)
            {
                return OwnedBinding {
                    proof: owner
                        .value
                        .proof
                        .project(&components, owner.value.ty.components.len()),
                    uniform: owner.value.uniform,
                    initialization: None,
                    place,
                };
            }
        }
        let projected = self.env.values().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            Some((owner, place.components_in(&owner.place)?))
        });
        if let Some((owner, components)) = projected {
            OwnedBinding {
                proof: owner
                    .proof
                    .project(&components, owner.place.ty().components.len()),
                uniform: owner.uniform,
                initialization: owner
                    .initialization
                    .as_ref()
                    .map(|ids| components.iter().map(|i| ids[*i].clone()).collect()),
                place,
            }
        } else {
            OwnedBinding {
                proof: self.projected_owned_proof(&place, None).unwrap_or_default(),
                place,
                uniform: false,
                initialization: None,
            }
        }
    }
}
