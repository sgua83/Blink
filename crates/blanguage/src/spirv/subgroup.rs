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
        let scope = self.constant(Scalar::U32, 3);
        let ty = self.ty(dst);
        let result = self.value(dst);
        match op {
            SubgroupOp::Lane | SubgroupOp::Size | SubgroupOp::Id => {
                self.emit(61, &[ty, result, self.subgroup_inputs[&op]]);
            }
            SubgroupOp::Elect => self.emit(333, &[ty, result, scope]),
            SubgroupOp::Any => self.emit(335, &[ty, result, scope, self.value(value.unwrap())]),
            SubgroupOp::All => self.emit(334, &[ty, result, scope, self.value(value.unwrap())]),
            SubgroupOp::BroadcastFirst => {
                self.emit(338, &[ty, result, scope, self.value(value.unwrap())])
            }
            SubgroupOp::Shuffle
            | SubgroupOp::ShuffleXor
            | SubgroupOp::ShuffleUp
            | SubgroupOp::ShuffleDown => {
                let opcode = if op == SubgroupOp::Shuffle {
                    345
                } else if op == SubgroupOp::ShuffleXor {
                    346
                } else if op == SubgroupOp::ShuffleUp {
                    347
                } else {
                    348
                };
                self.emit(
                    opcode,
                    &[
                        ty,
                        result,
                        scope,
                        self.value(value.unwrap()),
                        self.value(argument.unwrap()),
                    ],
                );
            }
            SubgroupOp::Count | SubgroupOp::PrefixCount => {
                let ballot = self.id();
                self.emit(
                    339,
                    &[self.ballot_type, ballot, scope, self.value(value.unwrap())],
                );
                self.emit(
                    342,
                    &[
                        ty,
                        result,
                        scope,
                        if op == SubgroupOp::PrefixCount { 2 } else { 0 },
                        ballot,
                    ],
                );
            }
            SubgroupOp::Sum | SubgroupOp::ExclusiveSum => {
                let opcode = if self.stage.registers[dst] == Scalar::F32 {
                    350
                } else {
                    349
                };
                self.emit(
                    opcode,
                    &[
                        ty,
                        result,
                        scope,
                        if op == SubgroupOp::ExclusiveSum { 2 } else { 0 },
                        self.value(value.unwrap()),
                    ],
                );
            }
            SubgroupOp::Min | SubgroupOp::Max => {
                let base = if op == SubgroupOp::Min { 353 } else { 356 };
                let offset = match self.stage.registers[dst] {
                    Scalar::I32 => 0,
                    Scalar::U32 => 1,
                    Scalar::F32 | Scalar::Bool => 2,
                };
                self.emit(
                    base + offset,
                    &[ty, result, scope, 0, self.value(value.unwrap())],
                );
            }
            SubgroupOp::BitAnd
            | SubgroupOp::BitOr
            | SubgroupOp::BitXor
            | SubgroupOp::ExclusiveBitAnd
            | SubgroupOp::ExclusiveBitOr
            | SubgroupOp::ExclusiveBitXor => {
                let opcode = if matches!(op, SubgroupOp::BitAnd | SubgroupOp::ExclusiveBitAnd) {
                    359
                } else if matches!(op, SubgroupOp::BitOr | SubgroupOp::ExclusiveBitOr) {
                    360
                } else {
                    361
                };
                let exclusive = matches!(
                    op,
                    SubgroupOp::ExclusiveBitAnd
                        | SubgroupOp::ExclusiveBitOr
                        | SubgroupOp::ExclusiveBitXor
                );
                self.emit(
                    opcode,
                    &[
                        ty,
                        result,
                        scope,
                        if exclusive { 2 } else { 0 },
                        self.value(value.unwrap()),
                    ],
                );
            }
        }
    }
}
