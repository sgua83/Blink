//! 原始CPU映像只消费原链接器已选择的COFF，不产生函数代码。
use super::{Inputs, Result, RootView, Roots, Source, combine, selection::select};
use crate::{names::ModuleName, native::ModuleCode, object::{Input, LinkSectionBytes}};
pub(crate) fn modules(inputs: Vec<Input>, borrowed: &[&crate::object::Object], roots: &[RootView<'_>], local: &[ModuleName<'_>], types: u32, programs: u32) -> Result<Option<ModuleCode>> {
    let mut linked = Inputs { sources: Vec::new(), libraries: Vec::new() };
    for input in inputs {
        linked.push(input);
    }
    linked.sources.extend(borrowed.iter().copied().map(Source::Borrowed));
    let objects = select(linked, Roots::Symbols(roots, types, programs), |bytes| LinkSectionBytes::Borrowed(bytes))?.objects;
    if objects.is_empty() { return Ok(None); }
    combine::module(objects, local).map(Some)
}
