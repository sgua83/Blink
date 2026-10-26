//! CPU 绑定持有独占数据或原冻结存储；两者共用同一原生执行入口。
use crate::{Dataset, DomainBindings, DomainStorage, ReadDataset, Result, error};

pub enum CpuDataset {
    Owned(Dataset),
    Read(ReadDataset),
}
impl From<Dataset> for CpuDataset {
    fn from(data: Dataset) -> Self {
        Self::Owned(data)
    }
}
impl From<ReadDataset> for CpuDataset {
    fn from(data: ReadDataset) -> Self {
        Self::Read(data)
    }
}
impl std::ops::Deref for CpuDataset {
    type Target = Dataset;
    fn deref(&self) -> &Dataset {
        match self {
            Self::Owned(data) => data,
            Self::Read(data) => data,
        }
    }
}
impl std::borrow::Borrow<Dataset> for CpuDataset {
    fn borrow(&self) -> &Dataset {
        self
    }
}
impl CpuDataset {
    pub fn freeze(self) -> Result<Self> {
        match self {
            Self::Owned(data) => data.freeze().map(Self::Read),
            Self::Read(_) => Ok(self),
        }
    }
    /// 原始可写访问仍只属于独占存储，并沿 Dataset 放弃旧 view 绑定。
    pub fn words_mut(&mut self) -> Result<&mut [u32]> {
        match self {
            Self::Owned(data) => Ok(data.words_mut()),
            Self::Read(_) => Err(error("冻结的 CPU 存储不能取得可写访问")),
        }
    }
}
impl DomainStorage for CpuDataset {
    fn read_identity(&self) -> Option<usize> {
        match self {
            Self::Owned(_) => None,
            Self::Read(data) => Some(data.identity()),
        }
    }
    fn read_binding(&self) -> Option<Self> {
        match self {
            Self::Owned(_) => None,
            Self::Read(data) => Some(Self::Read(data.clone())),
        }
    }
}
impl From<DomainBindings<Dataset>> for DomainBindings<CpuDataset> {
    fn from(bindings: DomainBindings<Dataset>) -> Self {
        Self {
            datasets: bindings
                .datasets
                .into_iter()
                .map(CpuDataset::Owned)
                .collect(),
            domains: bindings.domains,
        }
    }
}
impl DomainBindings<CpuDataset> {
    pub fn freeze(self, bindings: &[usize]) -> Result<Self> {
        self.freeze_with(bindings, CpuDataset::freeze)
    }
}
