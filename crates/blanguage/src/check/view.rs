//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
impl Builder<'_> {
    pub(super) fn view_length(view: &Value) -> Value {
        Value {
            proof: view.projected_proof(&[2]).with_domains(Domains::from([
                IndexDomain::ViewLength(view.view_identity()),
            ])),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![view.regs[2]],
            uniform: view.uniform,
            constant: None,
        }
    }
    pub(super) fn view_element(view: &Value, index: Reg) -> Place {
        let ty = view.ty.view.as_deref().unwrap().clone();
        let stride = ir::align_up(ty.size, ty.align);
        let words = ty.components.iter().map(|(_, offset)| offset / 4).collect();
        Place::ViewElement {
            ty,
            address: [view.regs[0], view.regs[1]],
            index,
            stride,
            words,
        }
    }
    pub(super) fn view_place(&mut self, base: &Expr, index: &Expr) -> Result<Place> {
        let view = if self.is_place(base) {
            let place = self.place(base)?;
            if place.ty().array.is_some() {
                return self.array_place(place, index);
            }
            self.read_place(&place, base.span)?
        } else {
            self.expr(base)?
        };
        if view.ty.array.is_some() {
            let place = self.materialize_array(view);
            return self.array_place(place, index);
        }
        self.view_value_place(view, index)
    }
    pub(super) fn view_value_place(&mut self, view: Value, index: &Expr) -> Result<Place> {
        if view.ty.view.is_none() {
            return self.fail(index.span, "B136", "索引需要 dataset、tile 或不可变 view");
        }
        let value = self.expr(index)?;
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
        if !self.index_in(
            &value,
            IndexDomain::View(view.view_identity()),
            Self::view_length(&view).minimum(),
        ) {
            return self.fail(
                index.span,
                "B137",
                "view 索引有效性无法静态证明；使用该引用值的长度条件或有界游标",
            );
        }
        Ok(Self::view_element(&view, value.regs[0]))
    }
}
