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
    /// 原冻结存储直接作为 CPU read 绑定，保留实际来源与子资源。
    pub fn binding(&self) -> crate::cpu::CpuDataset {
        crate::cpu::CpuDataset::Read(self.clone())
    }
}

/// 每个记录按类型声明顺序索引真实来源；执行地址由此图降低，不反向解析地址。
pub(crate) struct Views<Source> {
    pub sources: Vec<Source>,
    // 按实际存储字偏移排序；仍由同一引用图持有原source索引。
    pub bindings: Vec<(usize,usize)>,
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
    pub fn encode(&self,base_word:usize,words:&mut[u32],headers:&[[u32;6]]) {
        let start=self.bindings.partition_point(|(word,_)|*word<base_word);
        let end=base_word+words.len();
        for &(word,source)in self.bindings[start..].iter().take_while(|(word,_)|*word<end){
            let at=word-base_word;
            words[at..at+6].copy_from_slice(&headers[source]);
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

/// 原CPU记录中一个实际view绑定的只读借用；不复制或保活来源。
#[derive(Clone, Copy)]
pub struct DatasetView<'a> {
    pub record: u32,
    /// 相对本条记录起点的字节偏移，沿原layout解释。
    pub offset: u32,
    pub source: &'a Dataset,
}

impl Dataset {
    /// 枚举原绑定图的实际边；同源多字段返回同一个Dataset借用。
    /// 没有绑定的原始字节不成为来源，不解析其中的地址来恢复拥有关系。
    pub fn view_bindings(&self) -> impl ExactSizeIterator<Item = DatasetView<'_>> {
        self.views.bindings.iter().map(|&(word, source)| {
            let byte = word as u64 * 4;
            DatasetView {
                record: (byte / u64::from(self.stride)) as u32,
                offset: (byte % u64::from(self.stride)) as u32,
                source: &self.views.sources[source],
            }
        })
    }

    /// 按原字段路径和本条记录的实际活动载荷取得来源；无真实绑定时返回None。
    pub fn view_source(&self, record: u32, field: &str) -> Option<&Dataset> {
        if record >= self.count { return None; }
        let begin = record as usize * self.stride as usize;
        let words = &self.words[begin / 4..(begin + self.stride as usize).div_ceil(4)];
        let (offset, ty) = self.ty.field_at(field, words)?;
        ty.view.as_ref()?;
        let word = (begin + offset as usize) / 4;
        let binding = self.views.bindings.binary_search_by_key(&word, |&(word, _)| word).ok()?;
        Some(&self.views.sources[self.views.bindings[binding].1])
    }

    /// 交还不可变共享所有权；子资源随真实引用持续存活。
    pub fn freeze(self) -> Result<ReadDataset> {
        self.require_views()?;
        Ok(ReadDataset(Arc::new(self)))
    }

    /// 在原记录中编码主机引用，并持有去重的真实子资源；不复制子记录。
    /// 无view类型接受空绑定，保留原字节布局和记录数量。
    pub fn with_views(mut self, views: Vec<ViewBinding<'_>>) -> Result<Self> {
        let views = self.bind_views(views, |source| Ok((&source.0.ty, source.identity())))?;
        self.attach_views(views);
        Ok(self)
    }

    pub(crate) fn attach_views(&mut self, views: Views<ReadDataset>) {
        let headers: Vec<_> = views
            .sources
            .iter()
            .map(|source| {
                let address = source.0.words.as_ptr() as usize as u64;
                [address as u32, (address >> 32) as u32, source.0.count,source.0.stride,0,0]
            })
            .collect();
        views.encode(0,self.words_mut(),&headers);
        self.views = views;
    }

    pub(crate) fn bind_views<Source>(
        &self,
        views: Vec<ViewBinding<'_, Source>>,
        source: impl for<'a> Fn(&'a Source) -> Result<(&'a Ty, usize)>,
    ) -> Result<Views<Source>> {
        let stride=self.stride as usize/4;
        let mut references=Views{sources:Vec::new(),bindings:Vec::with_capacity(views.len())};
        if self.ty.contains_views() {
            for(record,words)in self.words.chunks(stride).enumerate(){
                self.ty.visit_view_fields(words,|offset,_|references.bindings.push((record*stride+offset as usize/4,usize::MAX))).map_err(error)?;
            }
        }
        if views.len()!=references.bindings.len(){return Err(error("view字段必须按各记录的实际活动载荷绑定真实不可变数据"));}
        let mut identities = BTreeMap::new();
        for view in views {
            if view.record >= self.count {return Err(error("view的记录范围与数据不一致"));}
            let begin=view.record as usize*stride;
            let (offset,ty)=self.ty.field_at(view.field,&self.words[begin..begin+stride]).ok_or_else(||error(format!("view字段{}不属于该记录的实际活动载荷",view.field)))?;
            let (element, identity) = source(&view.source)?;
            if ty.view.as_deref() != Some(element) {
                return Err(error("view 的元素类型与绑定存储不一致"));
            }
            let index=references.bindings.binary_search_by_key(&(begin+offset as usize/4),|(word,_)|*word).unwrap();
            let binding=&mut references.bindings[index].1;
            if *binding != usize::MAX {
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
            *binding = index;
        }
        Ok(references)
    }

    pub(crate) fn require_views(&self) -> Result<()> {
        if self.ty.contains_views() && self.views.sources.is_empty(){
            for words in self.words.chunks(self.stride as usize/4){
                let mut present=false;
                self.ty.visit_view_fields(words,|_,_|present=true).map_err(error)?;
                if present{return Err(error("CPU view数据缺少实际子资源所有权；原始字节不能代替with_views绑定"));}
            }
        }
        Ok(())
    }
}
