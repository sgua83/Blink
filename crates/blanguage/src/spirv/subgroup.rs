//! 按活动 invocation 集合协作；只产生值，不隐式同步设备存储。
use super::*;
impl Emitter<'_> {
    pub(super) fn subgroup(
        &mut self,
        dst: Reg,
        op: SubgroupOp,
        value: Option<Reg>,
        argument: Option<Reg>,
    ) {
        let scope_or_pointer = match op {
            SubgroupOp::Lane | SubgroupOp::Size | SubgroupOp::Id => self.subgroup_inputs[&op],
            _ => self.constant(Scalar::U32, 3),
        };
        let input = SubgroupEmission {
            operation: op as u32,
            scalar: self.stage.registers[dst].into(),
            result_type: self.ty(dst),
            result: self.value(dst),
            scope_or_pointer,
            value: value.map_or(0, |reg| self.value(reg)),
            argument: argument.map_or(0, |reg| self.value(reg)),
            ballot_type: self.ballot_type,
            next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(11);
        unsafe {
            let emitted = encode_subgroup(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
        }
    }
}
