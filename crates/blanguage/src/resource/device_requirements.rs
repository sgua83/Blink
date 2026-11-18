//! Physical scalar projections at the existing resource-image boundary.
use super::{DeviceRequirements, SubgroupCapabilities};
use crate::codec::{ReadStatus, contracts::{self, Native, Slot}};

#[repr(C)]
#[derive(Default)]
struct SubgroupFields {
    basic: u32,
    vote: u32,
    arithmetic: u32,
    ballot: u32,
    shuffle: u32,
    shuffle_relative: u32,
    extended_types: u32,
}
impl From<&SubgroupCapabilities> for SubgroupFields {
    fn from(value: &SubgroupCapabilities) -> Self {
        Self {
            basic: u32::from(value.basic),
            vote: u32::from(value.vote),
            arithmetic: u32::from(value.arithmetic),
            ballot: u32::from(value.ballot),
            shuffle: u32::from(value.shuffle),
            shuffle_relative: u32::from(value.shuffle_relative),
            extended_types: u32::from(value.extended_types),
        }
    }
}
impl From<SubgroupFields> for SubgroupCapabilities {
    fn from(value: SubgroupFields) -> Self {
        Self {
            basic: value.basic != 0,
            vote: value.vote != 0,
            arithmetic: value.arithmetic != 0,
            ballot: value.ballot != 0,
            shuffle: value.shuffle != 0,
            shuffle_relative: value.shuffle_relative != 0,
            extended_types: value.extended_types != 0,
        }
    }
}
unsafe extern "C" {
    #[link_name = "codec.value_format::contract_write_subgroup_capabilities"]
    fn write_subgroup_capabilities(value: SubgroupFields, output: Slot,
        append: unsafe extern "C" fn(Slot, *const u8, usize));
    #[link_name = "codec.value_format::contract_read_subgroup_capabilities"]
    fn read_subgroup_capabilities(input: contracts::Bytes, position: *mut usize, output: *mut SubgroupFields) -> ReadStatus;
}
impl Native for SubgroupCapabilities {
    fn empty() -> Self { Self::default() }
    unsafe fn write(&self, output: Slot) {
        unsafe { write_subgroup_capabilities(SubgroupFields::from(self), output, contracts::append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { contracts::input(context) };
        let mut value = SubgroupFields::default();
        let status = unsafe { read_subgroup_capabilities(bytes, position, &mut value) };
        if status == ReadStatus::Success { *output = value.into(); }
        status
    }
}
contracts::codec!(SubgroupCapabilities);

#[repr(C)]
#[derive(Default)]
struct DeviceFields {
    ray_query: u32,
    float64: u32,
    integer8: u32,
    integer16: u32,
    atomic64: u32,
    shared_atomic64: u32,
    float32_preserve: u32,
    float64_preserve: u32,
    sorting: u32,
    maximal_reconvergence: u32,
    texture_indexing: u32,
    position_fetch: u32,
    subgroup: SubgroupFields,
}
impl From<&DeviceRequirements> for DeviceFields {
    fn from(value: &DeviceRequirements) -> Self {
        Self {
            ray_query: u32::from(value.ray_query),
            float64: u32::from(value.float64),
            integer8: u32::from(value.integer8),
            integer16: u32::from(value.integer16),
            atomic64: u32::from(value.atomic64),
            shared_atomic64: u32::from(value.shared_atomic64),
            float32_preserve: u32::from(value.float32_preserve),
            float64_preserve: u32::from(value.float64_preserve),
            sorting: u32::from(value.sorting),
            maximal_reconvergence: u32::from(value.maximal_reconvergence),
            texture_indexing: u32::from(value.texture_indexing),
            position_fetch: u32::from(value.position_fetch),
            subgroup: SubgroupFields::from(&value.subgroup),
        }
    }
}
impl From<DeviceFields> for DeviceRequirements {
    fn from(value: DeviceFields) -> Self {
        Self {
            ray_query: value.ray_query != 0,
            float64: value.float64 != 0,
            integer8: value.integer8 != 0,
            integer16: value.integer16 != 0,
            atomic64: value.atomic64 != 0,
            shared_atomic64: value.shared_atomic64 != 0,
            float32_preserve: value.float32_preserve != 0,
            float64_preserve: value.float64_preserve != 0,
            sorting: value.sorting != 0,
            maximal_reconvergence: value.maximal_reconvergence != 0,
            texture_indexing: value.texture_indexing != 0,
            position_fetch: value.position_fetch != 0,
            subgroup: value.subgroup.into(),
        }
    }
}
unsafe extern "C" {
    #[link_name = "codec.value_format::contract_write_device_requirements"]
    fn write_device_requirements(value: DeviceFields, output: Slot,
        append: unsafe extern "C" fn(Slot, *const u8, usize));
    #[link_name = "codec.value_format::contract_read_device_requirements"]
    fn read_device_requirements(input: contracts::Bytes, position: *mut usize, output: *mut DeviceFields) -> ReadStatus;
}
impl Native for DeviceRequirements {
    fn empty() -> Self { Self::default() }
    unsafe fn write(&self, output: Slot) {
        unsafe { write_device_requirements(DeviceFields::from(self), output, contracts::append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { contracts::input(context) };
        let mut value = DeviceFields::default();
        let status = unsafe { read_device_requirements(bytes, position, &mut value) };
        if status == ReadStatus::Success { *output = value.into(); }
        status
    }
}
contracts::codec!(DeviceRequirements);

