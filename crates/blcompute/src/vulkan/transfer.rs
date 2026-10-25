//! 显式传输闭合资源图；一次操作内保留共享，不解析源平台地址或维护跨操作缓存。
use super::*;
use std::collections::BTreeMap;

impl Device {
    pub(super) fn upload_graph(
        &self,
        data: &Dataset,
        memo: &mut BTreeMap<usize, ReadDataset>,
    ) -> Result<GpuDataset> {
        let mut sources = Vec::with_capacity(data.views.sources.len());
        for source in &data.views.sources {
            let key = source.identity();
            let target = if let Some(target) = memo.get(&key) {
                target.clone()
            } else {
                let target = self.upload_graph(source, memo)?.freeze();
                memo.insert(key, target.clone());
                target
            };
            sources.push(target);
        }
        let mut target =
            GpuDataset::new(self.inner.clone(), data.ty.clone(), data.stride, data.count)?;
        target.storage.views = Views {
            sources,
            bindings: data.views.bindings.clone(),
        };
        self.upload_payload(&target, &data.words)?;
        Ok(target)
    }

    pub(super) fn download_graph(
        &self,
        data: &GpuDataset,
        memo: &mut BTreeMap<usize, crate::ReadDataset>,
    ) -> Result<Dataset> {
        let mut sources = Vec::with_capacity(data.views.sources.len());
        for source in &data.views.sources {
            let key = Rc::as_ptr(&source.0) as usize;
            let target = if let Some(target) = memo.get(&key) {
                target.clone()
            } else {
                let target = self.download_graph(&source.0, memo)?.freeze()?;
                memo.insert(key, target.clone());
                target
            };
            sources.push(target);
        }
        let mut target = Dataset {
            ty: data.ty.clone(),
            stride: data.stride,
            count: data.count,
            words: self.read_payload(data)?,
            views: Views::default(),
        };
        target.attach_views(Views {
            sources,
            bindings: data.views.bindings.clone(),
        });
        Ok(target)
    }
}
