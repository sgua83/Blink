//! 外部地址只解释为原CPU位置；实际存储和借用寿命仍由外部所有者保持。
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Identity {
    Constant(Bits),
    Value(Vec<ValueIdentity>),
}

impl PartialEq for ReferenceAddress {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Contiguous(a), Self::Contiguous(b)) => a == b,
            (Self::Components(a), Self::Components(b)) => a == b,
            (Self::External { identity: a, .. }, Self::External { identity: b, .. }) => a == b,
            _ => false,
        }
    }
}
impl ReferenceAddress {
    pub(super) fn overlaps(&self, ty: &Ty, words: &[u32], other: &Self, other_ty: &Ty, other_words: &[u32]) -> bool {
        let (base, other_base) = match (self, other) {
            (Self::External { identity: Identity::Constant(a), .. },
             Self::External { identity: Identity::Constant(b), .. }) => (u128::from(*a), u128::from(*b)),
            (Self::External { identity: Identity::Value(a), .. },
             Self::External { identity: Identity::Value(b), .. })
                if a.iter().any(|identity| b.contains(identity)) => (0, 0),
            _ if self == other => (0, 0),
            _ => return false,
        };
        ty.components.iter().zip(words).any(|((scalar, _), word)| {
            let begin = base + u128::from(*word);
            other_ty.components.iter().zip(other_words).any(|((other_scalar, _), other_word)| {
                let other_begin = other_base + u128::from(*other_word);
                begin < other_begin + u128::from(other_scalar.size())
                    && other_begin < begin + u128::from(scalar.size())
            })
        })
    }
}
impl Builder<'_> {
    pub(super) fn external_position(&mut self, explicit: &[GenericArgument], args: &[Expr], span: Span) -> Result<Option<Place>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B265", "unsafe_position只解释本进程CPU地址");
        }
        let ([GenericArgument::Type(name)], [arg]) = (explicit, args) else {
            return self.fail(span, "B153", "unsafe_position<T>需要一个明确普通类型和一个address实参");
        };
        let ty = self.world.ty(name, &self.definition.source, span, &self.definition.module, &self.definition.imports)?;
        if ty.owns_resources() {
            return self.fail(span, "B224", "外部地址位置不能创建Blink拥有资源");
        }
        if ty.contains_views() || ty.components.is_empty() {
            return self.fail(span, "B263", "外部地址位置需要非空普通值布局，不能解释Blink资源引用");
        }
        let Some(mut value) = self.expr(arg)? else { return Ok(None); };
        self.require_same(&Ty::builtin("address").unwrap(), &value.ty, arg.span)?;
        let identity = match bounds::Proof::capture(&value).known_value(&value.ty) {
            Some(bits) => Identity::Constant(bits[0]),
            None => Identity::Value(value.component_origins(0)),
        };
        value.ty = Ty::scalar(Scalar::U64);
        let low = self.cast(value.clone(), Scalar::U32).regs[0];
        let shift = self.constant(Scalar::U64, 32);
        let high = self.binary(">>", value, shift, span)?;
        let high = self.cast(high, Scalar::U32).regs[0];
        Ok(Some(Place::Reference {
            words: ty.components.iter().map(|(_, offset)| *offset).collect(),
            ty,
            address: ReferenceAddress::External { address: [low, high], identity },
            scope: None,
            access: Access::InOut,
        }))
    }
}
