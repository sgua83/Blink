//! Native import members use the same binding and COFF writer as other objects.
use super::*;
use crate::object::static_library::MemberView;
use std::{mem::MaybeUninit, ops::Range};

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeImportInput { dll: BindingBytes, name: BindingBytes, runtime: BindingBytes }

#[repr(C)]
struct NativeImportOutput {
    sections: *mut BindingSection,
    lengths: *mut u64,
    relocations: *mut BindingRelocation,
    associated: *mut usize,
    names: *const BindingBytes,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeImportPlan {
    slot_start: u64, slot_end: u64, member: u64, hint: u64, sections: u64, error: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.native_import::plan_import"]
    fn plan_import(input: NativeImportInput, output: NativeImportOutput) -> NativeImportPlan;
    #[link_name = "bootstrap.native_import::encode_member_name"]
    fn encode_member_name(input: NativeImportInput, plan: NativeImportPlan, output: *mut u8);
    #[link_name = "bootstrap.native_import::encode_section"]
    fn encode_section(input: NativeImportInput, plan: NativeImportPlan, section: u64, length: u64, output: *mut u8);
}

struct PlannedImport {
    input: NativeImportInput,
    plan: NativeImportPlan,
}

pub(super) struct Payload<'a> {
    import: &'a PlannedImport,
    section: u32,
    length: u32,
}

impl Payload<'_> {
    pub(super) fn len(&self) -> usize { self.length as usize }

    pub(super) unsafe fn write(&self, output: *mut u8) {
        unsafe {
            encode_section(self.import.input, self.import.plan, u64::from(self.section), u64::from(self.length), output)
        };
    }
}

pub(crate) struct NativeImportMember {
    name: String,
    bytes: Vec<u8>,
    slot: Range<usize>,
}

impl NativeImportMember {
    pub(crate) fn view(&self) -> MemberView<'_> {
        MemberView::new(&self.name, &self.bytes)
    }

    pub(crate) fn symbol(&self) -> &str {
        &self.name[self.slot.clone()]
    }
}

pub(crate) fn member(import: ImportKey<'_>) -> Result<NativeImportMember> {
    let input = NativeImportInput {
        dll: BindingBytes::new(import.dll().as_bytes()),
        name: BindingBytes::new(import.symbol().as_bytes()),
        runtime: BindingBytes::new(crate::application::RUNTIME_LIBRARY.as_bytes()),
    };
    let names = [".idata$2", ".idata$3", ".idata$4", ".idata$5", ".idata$6", ".rdata"]
        .map(|name| BindingBytes::new(name.as_bytes()));
    let mut bindings = Vec::with_capacity(5);
    let mut lengths = [0; 5];
    let mut relocations = [const { MaybeUninit::<BindingRelocation>::uninit() }; 5];
    let mut associated = [0; 4];
    let output = NativeImportOutput {
        sections: bindings.as_mut_ptr(), lengths: lengths.as_mut_ptr(),
        relocations: relocations.as_mut_ptr().cast(), associated: associated.as_mut_ptr(),
        names: names.as_ptr(),
    };
    let plan = unsafe { plan_import(input, output) };
    match plan.error {
        0 => (),
        1 => return Err("原生导入材料超出COFF格式范围".into()),
        code => return Err(format!("原生导入布局失败 {code}").into()),
    }
    let slot = plan.slot_start as usize..plan.slot_end as usize;
    let mut member = Vec::with_capacity(plan.member as usize);
    let member = unsafe {
        // B initialized the actual binding prefix and the one final member name.
        bindings.set_len(plan.sections as usize);
        encode_member_name(input, plan, member.as_mut_ptr());
        member.set_len(plan.member as usize);
        String::from_utf8_unchecked(member)
    };
    let import = PlannedImport { input, plan };
    let sections = bindings.iter().zip(lengths).enumerate().map(|(section, (row, length))| {
        // The B plan selected a name from the original static name table.
        let name = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(row.name.bytes, row.name.length))
        };
        let payload = Payload { import: &import, section: section as u32, length: length as u32 };
        (name, CoffPayload::NativeImport(payload), row.flags as u32)
    }).collect();
    let definition = BindingDefinition {
        name: BindingBytes::new(member[slot.clone()].as_bytes()), section: 0, value: 0, function: 0,
    };
    let bytes = encode_bound(&member[..slot.end], sections, bindings, &[definition], None)?;
    Ok(NativeImportMember { name: member, bytes, slot })
}
