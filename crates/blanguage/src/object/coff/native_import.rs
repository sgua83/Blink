//! Native import members use the same binding and COFF writer as other objects.
use super::*;
use crate::object::static_library::MemberView;
use std::{mem::MaybeUninit, ops::Range};

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeImportInput { dll: BindingBytes, name: BindingBytes, runtime: BindingBytes }

#[repr(C)]
struct NativeImportOutput {
    sections: *mut CoffSectionView,
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
}

#[repr(C)]
struct PlannedImport {
    input: NativeImportInput,
    plan: NativeImportPlan,
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
    let mut sections = Vec::with_capacity(5);
    let mut relocations = [const { MaybeUninit::<BindingRelocation>::uninit() }; 5];
    let mut associated = [0; 4];
    let output = NativeImportOutput {
        sections: sections.as_mut_ptr(),
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
        sections.set_len(plan.sections as usize);
        encode_member_name(input, plan, member.as_mut_ptr());
        member.set_len(plan.member as usize);
        String::from_utf8_unchecked(member)
    };
    let import = PlannedImport { input, plan };
    for section in &mut sections {
        section.body = std::ptr::from_ref(&import).cast();
        section.body_kind = CoffBody::NativeImport;
    }
    let definition = BindingDefinition {
        name: BindingBytes::new(member[slot.clone()].as_bytes()), section: 0, value: 0, function: 0,
    };
    let bytes = encode_coff(
        &member[..slot.end], sections.as_mut_ptr(), sections.len(), sections.len(),
        None::<std::iter::Empty<CoffSectionView>>, 1, std::iter::once(definition), None, None, owned_output,
    )?;
    Ok(NativeImportMember { name: member, bytes, slot })
}
