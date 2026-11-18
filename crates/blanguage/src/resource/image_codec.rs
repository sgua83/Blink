//! Physical typed construction for the existing B resource-image format.
use super::*;
use crate::codec::{
    ReadStatus,
    contracts::{self, Native, Read, Slot, Write, projected_record},
};
use std::collections::BTreeMap;

unsafe fn read_variant<T: Native, E>(
    context: Slot,
    output: Slot,
    construct: impl FnOnce(T) -> E,
) -> ReadStatus {
    let mut value = T::empty();
    let status = unsafe { T::read(context, &mut value) };
    if status == ReadStatus::Success {
        unsafe {
            *output.cast::<E>() = construct(value);
        }
    }
    status
}
macro_rules! write_variant {
    ($value:expr,$output:expr,$payload:ty,$symbol:literal) => {{
        unsafe extern "C" {
            #[link_name=$symbol]
            fn write(
                value: contracts::Address,
                output: Slot,
                payload: Write,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe {
            write(
                contracts::address($value),
                $output,
                contracts::write_value::<$payload>,
                contracts::append,
            )
        }
    }};
}
projected_record!(@ [],crate::ast::ExternalName,"codec.value_format::resource_write_external_name","codec.value_format::resource_read_external_name",{
library:String,
symbol:String,
},{});
projected_record!(@ [],native::ModuleCode,"codec.value_format::resource_write_module_code","codec.value_format::resource_read_module_code",{
sections:Vec<native::ModuleSection>,
symbols:BTreeMap<String,(usize,u32)>,
},{});
projected_record!(@ [],native::ModuleSection,"codec.value_format::resource_write_module_section","codec.value_format::resource_read_module_section",{
name:String,
bytes:Vec<u8>,
flags:u32,
relocations:Vec<native::ModuleRelocation>,
},{});
projected_record!(@ [],native::ModuleRelocation,"codec.value_format::resource_write_module_relocation","codec.value_format::resource_read_module_relocation",{
offset:usize,
kind:u16,
target:native::ModuleTarget,
},{});
projected_record!(@ [],native::Relocation,"codec.value_format::resource_write_native_relocation","codec.value_format::resource_read_native_relocation",{
offset:usize,
target:native::NativeSymbol,
},{});
projected_record!(@ [],native::NativeCode,"codec.value_format::resource_write_native_code","codec.value_format::resource_read_native_code",{
bytes:Vec<u8>,
relocations:Vec<native::Relocation>,
stack_bytes:u32,
calls:Vec<Option<usize>>,
function_end:usize,
unwind:Vec<u8>,
},{
#[cfg(feature = "compiler")] tail_regions: None,
#[cfg(feature = "compiler")] tail_references: None,
#[cfg(feature = "compiler")] sources: native::CodeSources::Unavailable,
});
projected_record!(@ [C],native::SortCode<C>,"codec.value_format::resource_write_sort_code","codec.value_format::resource_read_sort_code",{
source:usize,
target:usize,
initialize:Vec<C>,
histogram:C,
scatter:C,
gather:C,
},{});
projected_record!(@ [],spirv::ShaderCode,"codec.value_format::resource_write_shader_code","codec.value_format::resource_read_shader_code",{
words:Vec<u32>,
ranks:bool,
},{});
projected_record!(@ [],spirv::StageCode,"codec.value_format::resource_write_gpu_stage_code","codec.value_format::resource_read_gpu_stage_code",{
main:Vec<u32>,
tail:Option<Vec<u32>>,
scan_add:Option<Vec<u32>>,
finish:Option<(String,Vec<u32>)>,
dispatch:Option<Vec<u32>>,
selector:Option<Vec<u32>>,
sort:Option<Vec<spirv::ShaderCode>>,
},{});
projected_record!(@ [],Module<native::NativeCode>,"codec.value_format::resource_write_module","codec.value_format::resource_read_module",{
types:Vec<Ty>,
images:Vec<Image<TemplateProgram,native::NativeCode>>,
},{});
projected_record!(@ [],Module<usize>,"codec.value_format::resource_write_module","codec.value_format::resource_read_module",{
types:Vec<Ty>,
images:Vec<Image<TemplateProgram,usize>>,
},{});

const _: () = {
    type Value = Image<TemplateProgram, native::NativeCode>;
    #[repr(C)]
    struct Variants {
        cpu: Read,
        gpu: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_image"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn cpu(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<
                native::ProgramImage<TemplateProgram>,
                Image<TemplateProgram, native::NativeCode>,
            >(context, output, Value::Cpu)
        }
    }
    unsafe extern "C" fn gpu(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<
                spirv::ProgramImage<TemplateProgram>,
                Image<TemplateProgram, native::NativeCode>,
            >(context, output, Value::Gpu)
        }
    }
    impl Native for Image<TemplateProgram, native::NativeCode> {
        fn empty() -> Self {
            Self::Cpu(<native::ProgramImage<TemplateProgram> as Native>::empty())
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::Cpu(value) => write_variant!(
                    value,
                    output,
                    native::ProgramImage<TemplateProgram>,
                    "codec.value_format::resource_write_image_cpu"
                ),
                Self::Gpu(value) => write_variant!(
                    value,
                    output,
                    spirv::ProgramImage<TemplateProgram>,
                    "codec.value_format::resource_write_image_gpu"
                ),
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants { cpu: cpu, gpu: gpu },
                )
            }
        }
    }
    impl crate::codec::Encode for Image<TemplateProgram, native::NativeCode> {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl crate::codec::Decode for Image<TemplateProgram, native::NativeCode> {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};

const _: () = {
    type Value = Image<TemplateProgram, usize>;
    #[repr(C)]
    struct Variants {
        cpu: Read,
        gpu: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_linked_image"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn cpu(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<
                native::ProgramImage<TemplateProgram, usize>,
                Image<TemplateProgram, usize>,
            >(context, output, Value::Cpu)
        }
    }
    unsafe extern "C" fn gpu(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<spirv::ProgramImage<TemplateProgram>, Image<TemplateProgram, usize>>(
                context,
                output,
                Value::Gpu,
            )
        }
    }
    impl Native for Image<TemplateProgram, usize> {
        fn empty() -> Self {
            Self::Cpu(<native::ProgramImage<TemplateProgram, usize> as Native>::empty())
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::Cpu(value) => {
                    write_variant!(value,output,native::ProgramImage<TemplateProgram,usize>,"codec.value_format::resource_write_linked_image_cpu")
                }
                Self::Gpu(value) => write_variant!(
                    value,
                    output,
                    spirv::ProgramImage<TemplateProgram>,
                    "codec.value_format::resource_write_image_gpu"
                ),
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants { cpu: cpu, gpu: gpu },
                )
            }
        }
    }
    impl crate::codec::Encode for Image<TemplateProgram, usize> {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl crate::codec::Decode for Image<TemplateProgram, usize> {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};

const _: () = {
    type Value = native::ModuleTarget;
    #[repr(C)]
    struct Variants {
        section: Read,
        import: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_module_target"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn section(context: Slot, output: Slot) -> ReadStatus {
        #[repr(C)]
        struct Fields {
            section: Slot,
            value: Slot,
        }
        #[repr(C)]
        struct Reader {
            section: Read,
            value: Read,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::resource_read_module_target_section_fields"]
            fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
        }
        let mut section = <usize as Native>::empty();
        let mut value = <u32 as Native>::empty();
        let status = unsafe {
            read(
                context,
                Fields {
                    section: contracts::slot(&mut section),
                    value: contracts::slot(&mut value),
                },
                Reader {
                    section: contracts::read_value::<usize>,
                    value: contracts::read_value::<u32>,
                },
            )
        };
        if status == ReadStatus::Success {
            unsafe {
                *output.cast::<native::ModuleTarget>() = Value::Section { section, value };
            }
        }
        status
    }
    unsafe extern "C" fn import(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<crate::ast::ExternalName, native::ModuleTarget>(
                context,
                output,
                Value::Import,
            )
        }
    }
    impl Native for native::ModuleTarget {
        fn empty() -> Self {
            Self::Section {
                section: <usize as Native>::empty(),
                value: <u32 as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::Section { section, value } => {
                    #[repr(C)]
                    struct Fields {
                        section: Slot,
                        value: Slot,
                    }
                    #[repr(C)]
                    struct Writer {
                        section: Write,
                        value: Write,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::resource_write_module_target_section"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Writer,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            output,
                            Fields {
                                section: contracts::address(section).cast_mut(),
                                value: contracts::address(value).cast_mut(),
                            },
                            Writer {
                                section: contracts::write_value::<usize>,
                                value: contracts::write_value::<u32>,
                            },
                            contracts::append,
                        );
                    }
                }
                Self::Import(value) => write_variant!(
                    value,
                    output,
                    crate::ast::ExternalName,
                    "codec.value_format::resource_write_module_target_import"
                ),
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants {
                        section: section,
                        import: import,
                    },
                )
            }
        }
    }
    impl crate::codec::Encode for native::ModuleTarget {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl crate::codec::Decode for native::ModuleTarget {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};

const _: () = {
    type Value = native::NativeSymbol;
    #[repr(C)]
    struct Variants {
        print: Read,
        external: Read,
        function: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_native_symbol"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn print(context: Slot, output: Slot) -> ReadStatus {
        unsafe { read_variant::<Scalar, native::NativeSymbol>(context, output, Value::Print) }
    }
    unsafe extern "C" fn external(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<crate::ast::ExternalName, native::NativeSymbol>(
                context,
                output,
                Value::External,
            )
        }
    }
    unsafe extern "C" fn function(context: Slot, output: Slot) -> ReadStatus {
        unsafe { read_variant::<String, native::NativeSymbol>(context, output, Value::Function) }
    }
    impl Native for native::NativeSymbol {
        fn empty() -> Self {
            Self::Print(<Scalar as Native>::empty())
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::Print(value) => write_variant!(
                    value,
                    output,
                    Scalar,
                    "codec.value_format::resource_write_native_symbol_print"
                ),
                Self::External(value) => write_variant!(
                    value,
                    output,
                    crate::ast::ExternalName,
                    "codec.value_format::resource_write_native_symbol_external"
                ),
                Self::Function(value) => write_variant!(
                    value,
                    output,
                    String,
                    "codec.value_format::resource_write_native_symbol_function"
                ),
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants {
                        print: print,
                        external: external,
                        function: function,
                    },
                )
            }
        }
    }
    impl crate::codec::Encode for native::NativeSymbol {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl crate::codec::Decode for native::NativeSymbol {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};

const _: () = {
    type Value = native::RangeResult;
    #[repr(C)]
    struct Variants {
        none: Read,
        values: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_range_result"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn none(context: Slot, output: Slot) -> ReadStatus {
        let _ = context;
        unsafe {
            *output.cast::<native::RangeResult>() = Value::None;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn values(context: Slot, output: Slot) -> ReadStatus {
        unsafe { read_variant::<usize, native::RangeResult>(context, output, Value::Values) }
    }
    impl Native for native::RangeResult {
        fn empty() -> Self {
            Self::None
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::None => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::resource_write_range_result_none"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(output, contracts::append);
                    }
                }
                Self::Values(value) => write_variant!(
                    value,
                    output,
                    usize,
                    "codec.value_format::resource_write_range_result_values"
                ),
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants {
                        none: none,
                        values: values,
                    },
                )
            }
        }
    }
    impl crate::codec::Encode for native::RangeResult {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl crate::codec::Decode for native::RangeResult {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};

const _: () = {
    type Value<C> = native::ContinuationCode<C>;
    #[repr(C)]
    struct Variants {
        reduction_tail: Read,
        scan_add: Read,
        compact_scatter: Read,
        segmented_finish: Read,
        scan: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_continuation_code"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn reduction_tail<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<C, native::ContinuationCode<C>>(
                context,
                output,
                Value::<C>::ReductionTail,
            )
        }
    }
    unsafe extern "C" fn scan_add<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<C, native::ContinuationCode<C>>(context, output, Value::<C>::ScanAdd)
        }
    }
    unsafe extern "C" fn compact_scatter<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<C, native::ContinuationCode<C>>(
                context,
                output,
                Value::<C>::CompactScatter,
            )
        }
    }
    unsafe extern "C" fn segmented_finish<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<C, native::ContinuationCode<C>>(
                context,
                output,
                Value::<C>::SegmentedFinish,
            )
        }
    }
    unsafe extern "C" fn scan<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        #[repr(C)]
        struct Fields {
            prefix: Slot,
            apply: Slot,
            source: Slot,
            target: Slot,
        }
        #[repr(C)]
        struct Reader {
            prefix: Read,
            apply: Read,
            source: Read,
            target: Read,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::resource_read_continuation_code_scan_fields"]
            fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
        }
        let mut prefix = <C as Native>::empty();
        let mut apply = <C as Native>::empty();
        let mut source = <usize as Native>::empty();
        let mut target = <usize as Native>::empty();
        let status = unsafe {
            read(
                context,
                Fields {
                    prefix: contracts::slot(&mut prefix),
                    apply: contracts::slot(&mut apply),
                    source: contracts::slot(&mut source),
                    target: contracts::slot(&mut target),
                },
                Reader {
                    prefix: contracts::read_value::<C>,
                    apply: contracts::read_value::<C>,
                    source: contracts::read_value::<usize>,
                    target: contracts::read_value::<usize>,
                },
            )
        };
        if status == ReadStatus::Success {
            unsafe {
                *output.cast::<native::ContinuationCode<C>>() = Value::<C>::Scan {
                    prefix,
                    apply,
                    source,
                    target,
                };
            }
        }
        status
    }
    impl<C: Native> Native for native::ContinuationCode<C> {
        fn empty() -> Self {
            Self::ReductionTail(<C as Native>::empty())
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::ReductionTail(value) => write_variant!(
                    value,
                    output,
                    C,
                    "codec.value_format::resource_write_continuation_code_reduction_tail"
                ),
                Self::ScanAdd(value) => write_variant!(
                    value,
                    output,
                    C,
                    "codec.value_format::resource_write_continuation_code_scan_add"
                ),
                Self::CompactScatter(value) => write_variant!(
                    value,
                    output,
                    C,
                    "codec.value_format::resource_write_continuation_code_compact_scatter"
                ),
                Self::SegmentedFinish(value) => write_variant!(
                    value,
                    output,
                    C,
                    "codec.value_format::resource_write_continuation_code_segmented_finish"
                ),
                Self::Scan {
                    prefix,
                    apply,
                    source,
                    target,
                } => {
                    #[repr(C)]
                    struct Fields {
                        prefix: Slot,
                        apply: Slot,
                        source: Slot,
                        target: Slot,
                    }
                    #[repr(C)]
                    struct Writer {
                        prefix: Write,
                        apply: Write,
                        source: Write,
                        target: Write,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::resource_write_continuation_code_scan"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Writer,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            output,
                            Fields {
                                prefix: contracts::address(prefix).cast_mut(),
                                apply: contracts::address(apply).cast_mut(),
                                source: contracts::address(source).cast_mut(),
                                target: contracts::address(target).cast_mut(),
                            },
                            Writer {
                                prefix: contracts::write_value::<C>,
                                apply: contracts::write_value::<C>,
                                source: contracts::write_value::<usize>,
                                target: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        );
                    }
                }
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants {
                        reduction_tail: reduction_tail::<C>,
                        scan_add: scan_add::<C>,
                        compact_scatter: compact_scatter::<C>,
                        segmented_finish: segmented_finish::<C>,
                        scan: scan::<C>,
                    },
                )
            }
        }
    }
    impl<C: Native> crate::codec::Encode for native::ContinuationCode<C> {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl<C: Native> crate::codec::Decode for native::ContinuationCode<C> {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};

const _: () = {
    type Value<C> = native::StageCode<C>;
    #[repr(C)]
    struct Variants {
        element: Read,
        sort: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_read_cpu_stage_code"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Variants,
        ) -> ReadStatus;
    }
    unsafe extern "C" fn element<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        #[repr(C)]
        struct Fields {
            main: Slot,
            continuation: Slot,
            result: Slot,
        }
        #[repr(C)]
        struct Reader {
            main: Read,
            continuation: Read,
            result: Read,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::resource_read_cpu_stage_code_element_fields"]
            fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
        }
        let mut main = <C as Native>::empty();
        let mut continuation = <Option<native::ContinuationCode<C>> as Native>::empty();
        let mut result = <native::RangeResult as Native>::empty();
        let status = unsafe {
            read(
                context,
                Fields {
                    main: contracts::slot(&mut main),
                    continuation: contracts::slot(&mut continuation),
                    result: contracts::slot(&mut result),
                },
                Reader {
                    main: contracts::read_value::<C>,
                    continuation: contracts::read_value::<Option<native::ContinuationCode<C>>>,
                    result: contracts::read_value::<native::RangeResult>,
                },
            )
        };
        if status == ReadStatus::Success {
            unsafe {
                *output.cast::<native::StageCode<C>>() = Value::<C>::Element {
                    main,
                    continuation,
                    result,
                };
            }
        }
        status
    }
    unsafe extern "C" fn sort<C: Native>(context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            read_variant::<native::SortCode<C>, native::StageCode<C>>(
                context,
                output,
                Value::<C>::Sort,
            )
        }
    }
    impl<C: Native> Native for native::StageCode<C> {
        fn empty() -> Self {
            Self::Element {
                main: <C as Native>::empty(),
                continuation: <Option<native::ContinuationCode<C>> as Native>::empty(),
                result: <native::RangeResult as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::Element {
                    main,
                    continuation,
                    result,
                } => {
                    #[repr(C)]
                    struct Fields {
                        main: Slot,
                        continuation: Slot,
                        result: Slot,
                    }
                    #[repr(C)]
                    struct Writer {
                        main: Write,
                        continuation: Write,
                        result: Write,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::resource_write_cpu_stage_code_element"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Writer,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            output,
                            Fields {
                                main: contracts::address(main).cast_mut(),
                                continuation: contracts::address(continuation).cast_mut(),
                                result: contracts::address(result).cast_mut(),
                            },
                            Writer {
                                main: contracts::write_value::<C>,
                                continuation: contracts::write_value::<
                                    Option<native::ContinuationCode<C>>,
                                >,
                                result: contracts::write_value::<native::RangeResult>,
                            },
                            contracts::append,
                        );
                    }
                }
                Self::Sort(value) => write_variant!(
                    value,
                    output,
                    native::SortCode<C>,
                    "codec.value_format::resource_write_cpu_stage_code_sort"
                ),
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (input, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    input,
                    position,
                    context,
                    contracts::slot(output),
                    Variants {
                        element: element::<C>,
                        sort: sort::<C>,
                    },
                )
            }
        }
    }
    impl<C: Native> crate::codec::Encode for native::StageCode<C> {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl<C: Native> crate::codec::Decode for native::StageCode<C> {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};
macro_rules! native_image {
    ($code:ty) => {
        const _: () = {
            #[repr(C)]
            struct Fields {
                plan: Slot,
                stages: Slot,
                functions: Slot,
                resources: Slot,
                module_code: Slot,
            }
            #[repr(C)]
            struct Writer {
                plan: Write,
                stages: Write,
                functions: Write,
                resources: Write,
                module_code: Write,
            }
            #[repr(C)]
            struct Reader {
                plan: Read,
                stages: Read,
                functions: Read,
                resources: Read,
                module_code: Read,
            }
            unsafe extern "C" {
                #[link_name = "codec.value_format::resource_write_native_image"]
                fn write(output: Slot, fields: Fields, host: Writer);
                #[link_name = "codec.value_format::resource_read_native_image"]
                fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
            }
            impl<P> Native for native::ProgramImage<P, $code> {
                fn empty() -> Self {
                    unsafe {
                        Self::from_compiler_output(
                            ExecutionPlan::empty(),
                            Vec::new(),
                            Vec::new(),
                            None,
                        )
                    }
                }
                unsafe fn write(&self, output: Slot) {
                    let stages = self.stages();
                    let functions = self.functions();
                    unsafe {
                        write(
                            output,
                            Fields {
                                plan: contracts::address(self.program()).cast_mut(),
                                stages: contracts::address(&stages).cast_mut(),
                                functions: contracts::address(&functions).cast_mut(),
                                resources: contracts::address(&self.resources).cast_mut(),
                                module_code: contracts::address(&self.module_code).cast_mut(),
                            },
                            Writer {
                                plan: contracts::write_value::<ExecutionPlan>,
                                stages: contracts::write_slice_value::<native::StageCode<$code>>,
                                functions: contracts::write_slice_value::<(String, $code)>,
                                resources: contracts::write_value::<Option<Module<$code>>>,
                                module_code: contracts::write_value::<Option<native::ModuleCode>>,
                            },
                        )
                    }
                }
                unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
                    let mut program = <ExecutionPlan as Native>::empty();
                    let mut stages = <Vec<native::StageCode<$code>> as Native>::empty();
                    let mut functions = <Vec<(String, $code)> as Native>::empty();
                    let mut resources = <Option<Module<$code>> as Native>::empty();
                    let mut module_code = <Option<native::ModuleCode> as Native>::empty();
                    let status = unsafe {
                        read(
                            context,
                            Fields {
                                plan: contracts::slot(&mut program),
                                stages: contracts::slot(&mut stages),
                                functions: contracts::slot(&mut functions),
                                resources: contracts::slot(&mut resources),
                                module_code: contracts::slot(&mut module_code),
                            },
                            Reader {
                                plan: contracts::read_value::<ExecutionPlan>,
                                stages: contracts::read_value::<Vec<native::StageCode<$code>>>,
                                functions: contracts::read_value::<Vec<(String, $code)>>,
                                resources: contracts::read_value::<Option<Module<$code>>>,
                                module_code: contracts::read_value::<Option<native::ModuleCode>>,
                            },
                        )
                    };
                    if status == ReadStatus::Success {
                        let mut image = unsafe {
                            Self::from_compiler_output(program, stages, functions, resources)
                        };
                        image.module_code = module_code;
                        *output = image;
                    }
                    status
                }
            }
            impl<P> crate::codec::Encode for native::ProgramImage<P, $code> {
                fn encode(&self, w: &mut crate::codec::Writer) {
                    contracts::encode(self, w)
                }
            }
            impl<P> crate::codec::Decode for native::ProgramImage<P, $code> {
                fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
                    contracts::decode(r)
                }
            }
        };
    };
}
native_image!(native::NativeCode);
native_image!(usize);
const _: () = {
    #[repr(C)]
    struct Fields {
        plan: Slot,
        stages: Slot,
    }
    #[repr(C)]
    struct Writer {
        plan: Write,
        stages: Write,
    }
    #[repr(C)]
    struct Reader {
        plan: Read,
        stages: Read,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::resource_write_gpu_image"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::resource_read_gpu_image"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl<P> Native for spirv::ProgramImage<P> {
        fn empty() -> Self {
            unsafe { Self::from_compiler_output(ExecutionPlan::empty(), Vec::new()) }
        }
        unsafe fn write(&self, output: Slot) {
            let stages = self.stages();
            unsafe {
                write(
                    output,
                    Fields {
                        plan: contracts::address(self.program()).cast_mut(),
                        stages: contracts::address(&stages).cast_mut(),
                    },
                    Writer {
                        plan: contracts::write_value::<ExecutionPlan>,
                        stages: contracts::write_slice_value::<spirv::StageCode>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut program = ExecutionPlan::empty();
            let mut stages = Vec::<spirv::StageCode>::new();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        plan: contracts::slot(&mut program),
                        stages: contracts::slot(&mut stages),
                    },
                    Reader {
                        plan: contracts::read_value::<ExecutionPlan>,
                        stages: contracts::read_value::<Vec<spirv::StageCode>>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = unsafe { Self::from_compiler_output(program, stages) };
            }
            status
        }
    }
    impl<P> crate::codec::Encode for spirv::ProgramImage<P> {
        fn encode(&self, w: &mut crate::codec::Writer) {
            contracts::encode(self, w)
        }
    }
    impl<P> crate::codec::Decode for spirv::ProgramImage<P> {
        fn decode(r: &mut crate::codec::Reader<'_>) -> codec::Result<Self> {
            contracts::decode(r)
        }
    }
};
