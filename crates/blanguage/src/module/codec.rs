//! 模块产物的接口编码。该字节合同随实际定义／引用交给对象与链接器。
use std::collections::{BTreeMap, BTreeSet};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(crate) struct Writer(pub Vec<u8>);
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        let end = self.position.checked_add(size).ok_or("模块接口长度溢出")?;
        let bytes = self.bytes.get(self.position..end).ok_or("模块接口被截断")?;
        self.position = end;
        Ok(bytes)
    }
    pub fn finish(self) -> Result<()> {
        if self.position != self.bytes.len() {
            return Err("模块接口有未消费内容".into());
        }
        Ok(())
    }
}
pub(crate) trait Encode {
    fn encode(&self, writer: &mut Writer);
    fn bytes(&self) -> Vec<u8> {
        let mut writer = Writer(Vec::new());
        self.encode(&mut writer);
        writer.0
    }
}
pub(crate) trait Decode: Sized {
    fn decode(reader: &mut Reader<'_>) -> Result<Self>;
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let value = Self::decode(&mut reader)?;
        reader.finish()?;
        Ok(value)
    }
}
macro_rules! integer{($($ty:ty),*)=>{$(impl Encode for $ty{fn encode(&self,w:&mut Writer){w.0.extend(self.to_le_bytes());}}impl Decode for $ty{fn decode(r:&mut Reader<'_>)->Result<Self>{Ok(Self::from_le_bytes(r.take(size_of::<Self>())?.try_into()?))}})*};}
integer!(u8, u32, u64);
impl Encode for usize {
    fn encode(&self, w: &mut Writer) {
        (*self as u64).encode(w);
    }
}
impl Decode for usize {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self::try_from(u64::decode(r)?)?)
    }
}
impl Encode for bool {
    fn encode(&self, w: &mut Writer) {
        u8::from(*self).encode(w);
    }
}
impl Decode for bool {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match u8::decode(r)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("模块布尔值编码错误".into()),
        }
    }
}
impl<T: Encode> Encode for [T] {
    fn encode(&self, w: &mut Writer) {
        self.len().encode(w);
        for value in self {
            value.encode(w);
        }
    }
}
impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, w: &mut Writer) {
        self.as_slice().encode(w);
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        (0..usize::decode(r)?).map(|_| T::decode(r)).collect()
    }
}
impl Encode for String {
    fn encode(&self, w: &mut Writer) {
        self.len().encode(w);
        w.0.extend(self.as_bytes());
    }
}
impl Decode for String {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let size = usize::decode(r)?;
        Ok(std::str::from_utf8(r.take(size)?)?.to_owned())
    }
}
impl<T: Encode> Encode for Option<T> {
    fn encode(&self, w: &mut Writer) {
        self.is_some().encode(w);
        if let Some(value) = self {
            value.encode(w);
        }
    }
}
impl<T: Decode> Decode for Option<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(if bool::decode(r)? {
            Some(T::decode(r)?)
        } else {
            None
        })
    }
}
impl<T: Encode> Encode for Box<T> {
    fn encode(&self, w: &mut Writer) {
        (**self).encode(w);
    }
}
impl<T: Decode> Decode for Box<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Box::new(T::decode(r)?))
    }
}
impl<T: Encode + Ord> Encode for BTreeSet<T> {
    fn encode(&self, w: &mut Writer) {
        self.len().encode(w);
        for value in self {
            value.encode(w);
        }
    }
}
impl<T: Decode + Ord> Decode for BTreeSet<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        (0..usize::decode(r)?).map(|_| T::decode(r)).collect()
    }
}
impl<K: Encode + Ord, V: Encode> Encode for BTreeMap<K, V> {
    fn encode(&self, w: &mut Writer) {
        self.len().encode(w);
        for (key, value) in self {
            key.encode(w);
            value.encode(w);
        }
    }
}
impl<K: Decode + Ord, V: Decode> Decode for BTreeMap<K, V> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        (0..usize::decode(r)?)
            .map(|_| Ok((K::decode(r)?, V::decode(r)?)))
            .collect()
    }
}
impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self, w: &mut Writer) {
        for value in self {
            value.encode(w);
        }
    }
}
impl<T: Decode, const N: usize> Decode for [T; N] {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok((0..N)
            .map(|_| T::decode(r))
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .ok()
            .unwrap())
    }
}
macro_rules! tuple{($($ty:ident:$index:tt),*)=>{impl<$($ty:Encode),*> Encode for ($($ty,)*){fn encode(&self,w:&mut Writer){$(self.$index.encode(w);)*}}impl<$($ty:Decode),*> Decode for ($($ty,)*){fn decode(r:&mut Reader<'_>)->Result<Self>{Ok(($($ty::decode(r)?,)*))}}};}
tuple!(A:0,B:1);
tuple!(A:0,B:1,C:2);
macro_rules! record{
    ($ty:ty{$($field:ident),*$(,)?})=>{
        impl $crate::module::codec::Encode for $ty{fn encode(&self,w:&mut $crate::module::codec::Writer){$($crate::module::codec::Encode::encode(&self.$field,w);)*}}
        impl $crate::module::codec::Decode for $ty{fn decode(r:&mut $crate::module::codec::Reader<'_>)->$crate::module::codec::Result<Self>{Ok(Self{$($field:$crate::module::codec::Decode::decode(r)?,)*})}}
    };
}
macro_rules! variants{
    ($ty:ty{$($tag:literal=>$variant:ident$(($($binding:ident),*))?$({$($field:ident),*})?),*$(,)?})=>{
        impl $crate::module::codec::Encode for $ty{fn encode(&self,w:&mut $crate::module::codec::Writer){match self{$(Self::$variant$(($($binding),*))?$({$($field),*})?=>{$crate::module::codec::Encode::encode(&($tag as u32),w);$($($crate::module::codec::Encode::encode($binding,w);)*)?$($($crate::module::codec::Encode::encode($field,w);)*)?}),*}}}
        impl $crate::module::codec::Decode for $ty{fn decode(r:&mut $crate::module::codec::Reader<'_>)->$crate::module::codec::Result<Self>{Ok(match <u32 as $crate::module::codec::Decode>::decode(r)?{$($tag=>Self::$variant$(( $( {let $binding=$crate::module::codec::Decode::decode(r)?;$binding} ),* ))?$({$($field:$crate::module::codec::Decode::decode(r)?),*})?),*,_=>return Err(concat!("模块枚举编码错误：",stringify!($ty)).into())})}}
    };
}
use crate::{ast, ir};
pub(crate) use {record, variants};
variants!(ast::Access{0=>Value,1=>Read,2=>Write,3=>InOut,4=>Atomic});
variants!(ast::Target{0=>Compute,1=>Cpu,2=>Gpu});
variants!(ir::Scalar{0=>F32,1=>U32,2=>I32,3=>Bool});
record!(ir::Ty {
    name,
    arguments,
    values,
    size,
    align,
    components,
    fields,
    view,
    array,
    enumeration,
    result,
    dataset
});
variants!(ir::CallParameter{0=>Value(ty),1=>Reference{ty,access}});
variants!(ast::ContractKind{0=>Requires,1=>Ensures});
variants!(ast::ContractPredicate{0=>Prefix{data,count,field},1=>Nonzero(name),2=>Nonempty(name),3=>Index{value,resource},4=>Contiguous(name)});
