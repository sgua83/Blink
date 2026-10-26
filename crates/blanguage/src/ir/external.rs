//! C ABI 使用真实位置借用；普通值按目标 ABI 打包，不携带源语言资源引用。
use super::*;
#[derive(Clone, Debug)]
pub enum ExternalStorage {
    Local { base: usize, len: usize },
    Indexed(IndexedSource),
}
#[derive(Clone, Debug)]
pub struct ExternalReference {
    pub storage: ExternalStorage,
    pub offset: Option<Reg>,
    pub word: u32,
}
#[derive(Clone, Debug)]
pub enum ExternalArgument {
    Value {
        ty: Ty,
        values: Vec<Reg>,
    },
    Reference {
        ty: Ty,
        access: Access,
        address: ExternalReference,
    },
}
#[derive(Clone, Debug)]
pub struct ExternalCall {
    pub function: crate::ast::ExternalName,
    pub arguments: Vec<ExternalArgument>,
    pub result: Option<Ty>,
    pub values: Vec<Reg>,
}
impl ExternalReference {
    pub(crate) fn local_range(&self) -> Option<std::ops::Range<usize>> {
        match self.storage {
            ExternalStorage::Local { base, len } => Some(base..base + len),
            _ => None,
        }
    }
    pub(crate) fn operands(&self) -> Vec<Reg> {
        let mut regs = self.offset.into_iter().collect::<Vec<_>>();
        match &self.storage {
            ExternalStorage::Local { .. } => {}
            ExternalStorage::Indexed(
                IndexedSource::Data { index, .. } | IndexedSource::Tile { index, .. },
            ) => regs.push(*index),
            ExternalStorage::Indexed(IndexedSource::View { address, index, .. }) => {
                regs.extend(address);
                regs.push(*index);
            }
        }
        regs
    }
    pub(crate) fn map_operands(&mut self, r: &impl Fn(Reg) -> Reg) {
        self.offset = self.offset.map(r);
        match &mut self.storage {
            ExternalStorage::Local { .. } => {}
            ExternalStorage::Indexed(
                IndexedSource::Data { index, .. } | IndexedSource::Tile { index, .. },
            ) => *index = r(*index),
            ExternalStorage::Indexed(IndexedSource::View { address, index, .. }) => {
                for a in address {
                    *a = r(*a);
                }
                *index = r(*index);
            }
        }
    }
}
impl ExternalCall {
    pub(crate) fn operands(&self) -> Vec<Reg> {
        self.arguments
            .iter()
            .flat_map(|a| match a {
                ExternalArgument::Value { values, .. } => values.clone(),
                ExternalArgument::Reference { address, .. } => address.operands(),
            })
            .collect()
    }
    pub(crate) fn map_operands(&mut self, r: &impl Fn(Reg) -> Reg) {
        for argument in &mut self.arguments {
            match argument {
                ExternalArgument::Value { values, .. } => {
                    for v in values {
                        *v = r(*v)
                    }
                }
                ExternalArgument::Reference { address, .. } => address.map_operands(r),
            }
        }
    }
    pub(crate) fn locals(&self, writing: bool) -> Vec<usize> {
        self.arguments
            .iter()
            .flat_map(|arg| {
                if let ExternalArgument::Reference {
                    address, access, ..
                } = arg
                    && (if writing {
                        access.writes()
                    } else {
                        access.reads()
                    })
                {
                    address.local_range().into_iter().flatten().collect()
                } else {
                    Vec::new()
                }
            })
            .collect()
    }
}
