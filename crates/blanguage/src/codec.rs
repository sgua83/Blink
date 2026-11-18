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
    pub(crate) fn with_native_input<T>(&mut self,read:impl FnOnce(&[u8],&mut usize)->Result<T>)->Result<T>{
        read(self.bytes,&mut self.position)
    }
    pub fn text(&mut self) -> Result<&'a str> {
        let size = usize::decode(self)?;
        Ok(std::str::from_utf8(self.take(size)?)?)
    }
    pub fn byte_slice(&mut self) -> Result<&'a [u8]> {
        let size = usize::decode(self)?;
        // Match Vec<u8>'s truncated-payload error, including a maximum encoded count.
        let bytes = self.bytes[self.position..].get(..size).ok_or("模块接口被截断")?;
        self.position += size;
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
impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, writer: &mut Writer) { T::encode(*self, writer); }
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
integer!(u8, u16, u32, u64);
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
        Ok(r.text()?.to_owned())
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
impl<T: Encode> Encode for std::sync::Arc<T> {
    fn encode(&self,w:&mut Writer) { (**self).encode(w); }
}
impl<T: Decode> Decode for std::sync::Arc<T> {
    fn decode(r:&mut Reader<'_>)->Result<Self> { Ok(std::sync::Arc::new(T::decode(r)?)) }
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
impl<const N: usize> Encode for [u8; N] {
    fn encode(&self, w: &mut Writer) {
        w.0.extend_from_slice(self);
    }
}
impl<const N: usize> Decode for [u8; N] {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.take(N)?.try_into()?)
    }
}
macro_rules! tuple{($($ty:ident:$index:tt),*)=>{impl<$($ty:Encode),*> Encode for ($($ty,)*){fn encode(&self,w:&mut Writer){$(self.$index.encode(w);)*}}impl<$($ty:Decode),*> Decode for ($($ty,)*){fn decode(r:&mut Reader<'_>)->Result<Self>{Ok(($($ty::decode(r)?,)*))}}};}
tuple!(A:0,B:1);
tuple!(A:0,B:1,C:2);
macro_rules! record{
    ($(@<$($generic:ident),+>)? $ty:ty{$($field:ident),*$(,)?})=>{
        impl$(<$($generic: $crate::codec::Encode),+>)? $crate::codec::Encode for $ty{fn encode(&self,w:&mut $crate::codec::Writer){$($crate::codec::Encode::encode(&self.$field,w);)*}}
        impl$(<$($generic: $crate::codec::Decode),+>)? $crate::codec::Decode for $ty{fn decode(r:&mut $crate::codec::Reader<'_>)->$crate::codec::Result<Self>{Ok(Self{$($field:$crate::codec::Decode::decode(r)?,)*})}}
    };
}
macro_rules! variants{
    ($(@<$($generic:ident),+>)? $ty:ty{$($tag:literal=>$variant:ident$(($($binding:ident),*))?$({$($field:ident),*})?),*$(,)?})=>{
        impl$(<$($generic: $crate::codec::Encode),+>)? $crate::codec::Encode for $ty{fn encode(&self,w:&mut $crate::codec::Writer){match self{$(Self::$variant$(($($binding),*))?$({$($field),*})?=>{$crate::codec::Encode::encode(&($tag as u32),w);$($($crate::codec::Encode::encode($binding,w);)*)?$($($crate::codec::Encode::encode($field,w);)*)?}),*}}}
        impl$(<$($generic: $crate::codec::Decode),+>)? $crate::codec::Decode for $ty{fn decode(r:&mut $crate::codec::Reader<'_>)->$crate::codec::Result<Self>{Ok(match <u32 as $crate::codec::Decode>::decode(r)?{$($tag=>Self::$variant$(( $( {let $binding=$crate::codec::Decode::decode(r)?;$binding} ),* ))?$({$($field:$crate::codec::Decode::decode(r)?),*})?),*,_=>return Err(concat!("模块枚举编码错误：",stringify!($ty)).into())})}}
    };
}
use crate::{ast, ir};
pub(crate) use {record, variants};
pub(crate) mod types;
impl Encode for ir::Ty {
    fn encode(&self,writer:&mut Writer) { types::encode(self,writer); }
}
impl Decode for ir::Ty {
    fn decode(reader:&mut Reader<'_>)->Result<Self> {
        types::reading::decode(reader)
    }
}

record!(ast::Span { start, end });

pub(crate) use types::reading::ReadStatus;

pub(crate) mod contracts;

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(ir::Buffer);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        name: Slot,
        access: Slot,
        ty: Slot,
        stride: Slot,
        count: Slot,
    }
    #[repr(C)]
    struct Writer {
        name: WriteCallback,
        access: WriteCallback,
        ty: WriteCallback,
        stride: WriteCallback,
        count: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        name: ReadCallback,
        access: ReadCallback,
        ty: ReadCallback,
        stride: ReadCallback,
        count: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_write_buffer"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::contract_read_buffer"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for ir::Buffer {
        fn empty() -> Self {
            Self {
                name: <String as Native>::empty(),
                access: <ast::Access as Native>::empty(),
                ty: <ir::Ty as Native>::empty(),
                stride: <u32 as Native>::empty(),
                count: <Option<u32> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        name: contracts::address(&self.name).cast_mut(),
                        access: contracts::address(&self.access).cast_mut(),
                        ty: contracts::address(&self.ty).cast_mut(),
                        stride: contracts::address(&self.stride).cast_mut(),
                        count: contracts::address(&self.count).cast_mut(),
                    },
                    Writer {
                        name: contracts::write_value::<String>,
                        access: contracts::write_value::<ast::Access>,
                        ty: contracts::write_value::<ir::Ty>,
                        stride: contracts::write_value::<u32>,
                        count: contracts::write_value::<Option<u32>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        name: contracts::slot(&mut output.name),
                        access: contracts::slot(&mut output.access),
                        ty: contracts::slot(&mut output.ty),
                        stride: contracts::slot(&mut output.stride),
                        count: contracts::slot(&mut output.count),
                    },
                    Reader {
                        name: contracts::read_value::<String>,
                        access: contracts::read_value::<ast::Access>,
                        ty: contracts::read_value::<ir::Ty>,
                        stride: contracts::read_value::<u32>,
                        count: contracts::read_value::<Option<u32>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(ir::Uniform);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        name: Slot,
        ty: Slot,
        offset: Slot,
    }
    #[repr(C)]
    struct Writer {
        name: WriteCallback,
        ty: WriteCallback,
        offset: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        name: ReadCallback,
        ty: ReadCallback,
        offset: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_write_uniform"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::contract_read_uniform"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for ir::Uniform {
        fn empty() -> Self {
            Self {
                name: <String as Native>::empty(),
                ty: <ir::Ty as Native>::empty(),
                offset: <u32 as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        name: contracts::address(&self.name).cast_mut(),
                        ty: contracts::address(&self.ty).cast_mut(),
                        offset: contracts::address(&self.offset).cast_mut(),
                    },
                    Writer {
                        name: contracts::write_value::<String>,
                        ty: contracts::write_value::<ir::Ty>,
                        offset: contracts::write_value::<u32>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        name: contracts::slot(&mut output.name),
                        ty: contracts::slot(&mut output.ty),
                        offset: contracts::slot(&mut output.offset),
                    },
                    Reader {
                        name: contracts::read_value::<String>,
                        ty: contracts::read_value::<ir::Ty>,
                        offset: contracts::read_value::<u32>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(ast::Access);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        value: ReadCallback,
        read: ReadCallback,
        write: ReadCallback,
        inout: ReadCallback,
        atomic: ReadCallback,
    }
    unsafe extern "C" fn build_value(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Access>() = ast::Access::Value;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_read(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Access>() = ast::Access::Read;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_write(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Access>() = ast::Access::Write;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_inout(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Access>() = ast::Access::InOut;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_atomic(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Access>() = ast::Access::Atomic;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_access"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ast::Access {
        fn empty() -> Self {
            Self::Value
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Value => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_access_value"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Read => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_access_read"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Write => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_access_write"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::InOut => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_access_inout"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Atomic => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_access_atomic"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        value: build_value,
                        read: build_read,
                        write: build_write,
                        inout: build_inout,
                        atomic: build_atomic,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(ast::Target);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        compute: ReadCallback,
        cpu: ReadCallback,
        gpu: ReadCallback,
    }
    unsafe extern "C" fn build_compute(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Target>() = ast::Target::Compute;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_cpu(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Target>() = ast::Target::Cpu;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_gpu(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::Target>() = ast::Target::Gpu;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_target"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ast::Target {
        fn empty() -> Self {
            Self::Compute
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Compute => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_target_compute"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Cpu => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_target_cpu"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Gpu => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_target_gpu"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        compute: build_compute,
                        cpu: build_cpu,
                        gpu: build_gpu,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(ir::CallParameterKind);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        value: ReadCallback,
        reference: ReadCallback,
    }
    unsafe extern "C" fn build_value(context: Slot, destination: Slot) -> ReadStatus {
        let mut ty = <ir::Ty as Native>::empty();
        #[repr(C)]
        struct Fields {
            ty: Slot,
        }
        #[repr(C)]
        struct Host {
            ty: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_read_call_parameter_kind_value"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    ty: contracts::slot(&mut ty),
                },
                Host {
                    ty: contracts::read_value::<ir::Ty>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ir::CallParameterKind>() = ir::CallParameterKind::Value(ty);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_reference(context: Slot, destination: Slot) -> ReadStatus {
        let mut ty = <ir::Ty as Native>::empty();
        let mut access = <ast::Access as Native>::empty();
        #[repr(C)]
        struct Fields {
            ty: Slot,
            access: Slot,
        }
        #[repr(C)]
        struct Host {
            ty: ReadCallback,
            access: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_read_call_parameter_kind_reference"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    ty: contracts::slot(&mut ty),
                    access: contracts::slot(&mut access),
                },
                Host {
                    ty: contracts::read_value::<ir::Ty>,
                    access: contracts::read_value::<ast::Access>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ir::CallParameterKind>() =
                ir::CallParameterKind::Reference { ty, access };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_call_parameter_kind"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ir::CallParameterKind {
        fn empty() -> Self {
            Self::Value(crate::codec::contracts::empty())
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Value(ty) => {
                    #[repr(C)]
                    struct Fields {
                        ty: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        ty: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_call_parameter_kind_value"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                ty: contracts::address(ty).cast_mut(),
                            },
                            Host {
                                ty: contracts::write_value::<ir::Ty>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Reference { ty, access } => {
                    #[repr(C)]
                    struct Fields {
                        ty: Slot,
                        access: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        ty: WriteCallback,
                        access: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_call_parameter_kind_reference"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                ty: contracts::address(ty).cast_mut(),
                                access: contracts::address(access).cast_mut(),
                            },
                            Host {
                                ty: contracts::write_value::<ir::Ty>,
                                access: contracts::write_value::<ast::Access>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        value: build_value,
                        reference: build_reference,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(ast::ContractKind);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        requires: ReadCallback,
        ensures: ReadCallback,
    }
    unsafe extern "C" fn build_requires(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::ContractKind>() = ast::ContractKind::Requires;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_ensures(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ast::ContractKind>() = ast::ContractKind::Ensures;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_contract_kind"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ast::ContractKind {
        fn empty() -> Self {
            Self::Requires
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Requires => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_contract_kind_requires"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Ensures => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_contract_kind_ensures"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        requires: build_requires,
                        ensures: build_ensures,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(ir::Scalar);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        f32: ReadCallback,
        u32: ReadCallback,
        i32: ReadCallback,
        bool: ReadCallback,
        u64: ReadCallback,
        i64: ReadCallback,
        f64: ReadCallback,
        u8: ReadCallback,
        i8: ReadCallback,
        u16: ReadCallback,
        i16: ReadCallback,
    }
    unsafe extern "C" fn build_f32(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::F32;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_u32(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::U32;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_i32(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::I32;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_bool(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::Bool;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_u64(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::U64;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_i64(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::I64;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_f64(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::F64;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_u8(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::U8;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_i8(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::I8;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_u16(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::U16;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_i16(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ir::Scalar>() = ir::Scalar::I16;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_scalar"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ir::Scalar {
        fn empty() -> Self {
            Self::U32
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::F32 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_f32"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::U32 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_u32"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::I32 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_i32"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Bool => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_bool"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::U64 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_u64"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::I64 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_i64"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::F64 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_f64"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::U8 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_u8"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::I8 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_i8"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::U16 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_u16"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::I16 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_scalar_i16"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        f32: build_f32,
                        u32: build_u32,
                        i32: build_i32,
                        bool: build_bool,
                        u64: build_u64,
                        i64: build_i64,
                        f64: build_f64,
                        u8: build_u8,
                        i8: build_i8,
                        u16: build_u16,
                        i16: build_i16,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(ast::ContractPredicate);

crate::codec::contracts::codec!(ast::OutputCondition);

crate::codec::contracts::codec!(ir::CallParameter);

impl contracts::Native for ir::CallParameter {
    fn empty() -> Self {
        Self::new(contracts::empty())
    }
    unsafe fn write(&self, output: contracts::Slot) {
        unsafe { contracts::Native::write(&self.kind, output) }
    }
    unsafe fn read(context: contracts::Slot, output: &mut Self) -> ReadStatus {
        let mut kind = contracts::empty();
        let status =
            unsafe { <ir::CallParameterKind as contracts::Native>::read(context, &mut kind) };
        if status == ReadStatus::Success {
            *output = Self::new(kind);
        }
        status
    }
}

