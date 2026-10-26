//! 独占位置沿静态内联传递；字段只投影原槽／原缓冲字，不创建引用 ABI。
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Position {
    Local(usize),
    Element(usize, u32),
    Parameter(usize, usize),
}
impl Place {
    pub(super) fn owned_positions(&self) -> Option<Vec<Position>> {
        match self {
            Self::Indexed { base, .. } => base.owned_positions(),
            Self::ReadOnlyLocal(..) => Some(Vec::new()),
            Self::Local(_, slots, ..) => Some(slots.iter().copied().map(Position::Local).collect()),
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
            Self::AtomicParameter(ty)
            | Self::Local(ty, _, ..)
            | Self::ReadOnlyLocal(ty, _, ..)
            | Self::Indexed { ty, .. }
            | Self::Element { ty, .. }
            | Self::TileElement { ty, .. }
            | Self::ViewElement { ty, .. } => ty,
        }
    }
    pub(super) fn components_in(&self, owner: &Place) -> Option<Vec<usize>> {
        match (self, owner) {
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
    fn overlaps(&self, other: &Place) -> bool {
        match (self, other) {
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
    pub(super) fn mutable_place(&mut self, expr: &Expr, access: Access) -> Result<Place> {
        let place = self.place(expr)?;
        let positions = self.writable_storage(&place, expr.span)?;
        if positions.iter().copied().collect::<BTreeSet<_>>().len() != positions.len() {
            return self.fail(expr.span, "B227", "独占借用位置不能包含重复分量");
        }
        if access == Access::InOut {
            self.require_initialized(&place, expr.span)?;
        }
        Ok(place)
    }
    pub(super) fn writable_storage(&self, place: &Place, span: Span) -> Result<Vec<usize>> {
        if let Place::Indexed { base, words, .. } = place {
            self.writable_storage(base, span)?;
            return Ok(words.iter().map(|w| *w as usize).collect());
        }
        match place {
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
    fn positions(&self, place: &Place) -> Vec<Position> {
        if let Some(positions) = self.env.values().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            let identities = owner.initialization.as_ref()?;
            let components = place.components_in(&owner.place)?;
            Some(components.iter().map(|i| identities[*i]).collect())
        }) {
            return positions;
        }
        place.owned_positions().unwrap()
    }
    pub(super) fn is_initialized(&self, place: &Place) -> bool {
        self.positions(place)
            .iter()
            .all(|p| self.initialized.contains(p))
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
            yes.intersection(&no).copied().collect()
        };
    }
    pub(super) fn require_disjoint(
        &self,
        places: &[Place],
        place: &Place,
        span: Span,
    ) -> Result<()> {
        if places.iter().any(|p| p.overlaps(place)) {
            return self.fail(
                span,
                "B227",
                "同一次调用的 write／inout 借用必须互不重叠；整个记录和其字段也不能同时独占借用",
            );
        }
        Ok(())
    }
    pub(super) fn borrow_owned(&self, place: Place) -> OwnedBinding {
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
                    .map(|ids| components.iter().map(|i| ids[*i]).collect()),
                place,
            }
        } else {
            OwnedBinding {
                place,
                proof: bounds::Proof::default(),
                uniform: false,
                initialization: None,
            }
        }
    }
}
