//! 主机不可变存储持有真实子资源；CPU 物理地址只在绑定边界生成。
use crate::{Dataset, Result, error};
use blanguage::ir::Ty;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub struct ReadDataset(Arc<Dataset>);
impl ReadDataset {
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}

/// 每个记录按类型声明顺序索引真实来源；执行地址由此图降低，不反向解析地址。
pub(crate) struct Views<Source> {
    pub sources: Vec<Source>,
    pub bindings: Vec<usize>,
}
impl<Source> Default for Views<Source> {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            bindings: Vec::new(),
        }
    }
}
impl<Source> Views<Source> {
    pub fn clear(&mut self) {
        self.bindings.clear();
        self.sources.clear();
    }
    pub fn encode(
        &self,
        fields: &[u32],
        stride_words: usize,
        base_record: usize,
        words: &mut [u32],
        headers: &[[u32; 3]],
    ) {
        if fields.is_empty() {
            return;
        }
        for (record, words) in words.chunks_mut(stride_words).enumerate() {
            let start = (base_record + record) * fields.len();
            for (field, offset) in fields.iter().enumerate() {
                let at = *offset as usize / 4;
                words[at..at + 3].copy_from_slice(&headers[self.bindings[start + field]]);
            }
        }
    }
}

impl std::ops::Deref for ReadDataset {
    type Target = Dataset;
    fn deref(&self) -> &Dataset {
        &self.0
    }
}

pub struct ViewBinding<'a, Source = ReadDataset> {
    pub record: u32,
    pub field: &'a str,
    pub source: Source,
}

impl Dataset {
    /// 交还不可变共享所有权；子资源随真实引用持续存活。
    pub fn freeze(self) -> Result<ReadDataset> {
        self.require_views()?;
        Ok(ReadDataset(Arc::new(self)))
    }

    /// 在原记录中编码主机引用，并持有去重的真实子资源；不复制子记录。
    pub fn with_views(mut self, views: Vec<ViewBinding<'_>>) -> Result<Self> {
        let views = self.bind_views(views, |source| Ok((&source.0.ty, source.identity())))?;
        self.attach_views(views);
        Ok(self)
    }

    pub(crate) fn attach_views(&mut self, views: Views<ReadDataset>) {
        let fields: Vec<_> = self
            .ty
            .view_fields()
            .iter()
            .map(|(offset, _)| *offset)
            .collect();
        let headers: Vec<_> = views
            .sources
            .iter()
            .map(|source| {
                let address = source.0.words.as_ptr() as usize as u64;
                [address as u32, (address >> 32) as u32, source.0.count]
            })
            .collect();
        let stride = self.stride as usize / 4;
        views.encode(&fields, stride, 0, self.words_mut(), &headers);
        self.views = views;
    }

    pub(crate) fn bind_views<Source>(
        &self,
        views: Vec<ViewBinding<'_, Source>>,
        source: impl for<'a> Fn(&'a Source) -> Result<(&'a Ty, usize)>,
    ) -> Result<Views<Source>> {
        let fields: BTreeMap<_, _> = self
            .ty
            .view_fields()
            .iter()
            .enumerate()
            .map(|(i, (offset, _))| (*offset, i))
            .collect();
        if views.len() as u64 != fields.len() as u64 * self.count as u64 {
            return Err(error("view 字段必须逐记录绑定真实的不可变数据"));
        }
        let mut identities = BTreeMap::new();
        let mut references = Views {
            sources: Vec::new(),
            bindings: vec![usize::MAX; views.len()],
        };
        for view in views {
            let (offset, ty) = self
                .ty
                .field(view.field)
                .ok_or_else(|| error(format!("未知 view 字段 {}", view.field)))?;
            if view.record >= self.count {
                return Err(error("view 的记录范围与数据不一致"));
            }
            let (element, identity) = source(&view.source)?;
            if ty.view.as_deref() != Some(element) {
                return Err(error("view 的元素类型与绑定存储不一致"));
            }
            let binding = view.record as usize * fields.len() + fields[&offset];
            if references.bindings[binding] != usize::MAX {
                return Err(error("view 字段重复绑定"));
            }
            let index = if let Some(index) = identities.get(&identity) {
                *index
            } else {
                let index = references.sources.len();
                identities.insert(identity, index);
                references.sources.push(view.source);
                index
            };
            references.bindings[binding] = index;
        }
        Ok(references)
    }

    pub(crate) fn require_views(&self) -> Result<()> {
        if self.count != 0 && self.ty.contains_views() && self.views.sources.is_empty() {
            return Err(error(
                "CPU view 数据缺少真实子资源所有权；原始字节不能代替 with_views 绑定",
            ));
        }
        Ok(())
    }
}
