//! 返回点检查自己的输出与值；退出后只合流真实可继续路径，函数域不结束并行实例。
use super::*;
mod body;
mod exit;
mod recursive;

#[derive(Clone)]
pub(super) enum Returned {
    Unit,
    // 跨阶段结果移动唯一载荷；普通表达式仍直接使用 Value。
    Value(Box<Value>),
    Dataset(application::Data),
    OwnedValue(owned_values::OwnedValue),
}
#[derive(Clone)]
enum Expected {
    Void,
    Never,
    Value(Ty),
    Dataset { ty: Ty, count: Option<u32> },
}
impl Expected{
 fn kind(&self)->u32{match self{Self::Void=>0,Self::Never=>1,Self::Value(_)=>2,Self::Dataset{..}=>3}}
}
impl Returned {
 pub(super) fn kind(&self)->u32{match self{Self::Unit=>0,Self::Value(_)=>1,Self::Dataset(_)=>2,Self::OwnedValue(_)=>3}}
 fn register_count(&self)->usize{match self{Self::Unit=>0,Self::Value(value)=>value.regs.len(),Self::OwnedValue(value)=>value.value.regs.len(),Self::Dataset(_)=>2}}
 fn register(&self,index:usize)->Reg{match self{Self::Value(value)=>value.regs[index],Self::OwnedValue(value)=>value.value.regs[index],Self::Dataset(data)=>if index==0{data.root.regs[0]}else{data.length.regs[0]},Self::Unit=>unsafe{std::hint::unreachable_unchecked()}}}

    pub(super) fn value(value: Value) -> Self { Self::Value(Box::new(value)) }
    pub(super) fn view_value(&self)->Option<&Value>{match self{Self::Value(value)=>Some(value),Self::OwnedValue(value)=>Some(&value.value),_=>None}}
    pub(super) fn view_value_mut(&mut self)->Option<&mut Value>{match self{Self::Value(value)=>Some(value),Self::OwnedValue(value)=>Some(&mut value.value),_=>None}}
    pub(super) fn bind_outputs(
        &mut self,
        registers: Option<&[Reg]>,
        bindings: &bounds::OutputBindings,
        aliases: &bounds::Clause,
    ) {
        fn value(
            value: &mut Value,
            registers: Option<&[Reg]>,
            bindings: &bounds::OutputBindings,
            aliases: &bounds::Clause,
        ) {
            value.proof = bounds::Proof::bound_return(value, registers, bindings, aliases);
        }
        match self {
            Self::Unit => {}
            Self::Value(v) => value(v, registers, bindings, aliases),
            Self::OwnedValue(v) => {
                value(&mut v.value, registers, bindings, aliases);
                for payload in v.payloads.iter_mut().flatten() {
                    payload.bind_outputs(None, bindings, aliases);
                }
            }
            Self::Dataset(v) => {
                v.residency=v.residency.output_bound(bindings).with_aliases(aliases);
                for fact in v.prefixes.values_mut(){*fact=fact.output_bound(bindings);}
                value(&mut v.root, registers.map(|r| &r[..1]), bindings, aliases);
                value(&mut v.length, registers.map(|r| &r[1..]), bindings, aliases);
                v.elements=std::mem::take(&mut v.elements).map(&mut |mut elements|{
                    if let Some(value)=elements.view_value_mut(){
                        value.proof.origins.resize(value.regs.len());
                        for(component,origins)in value.proof.origins.iter_mut().enumerate(){if origins.is_empty(){origins.push(ValueIdentity::ElementComponent(component));}}
                    }
                    elements.bind_outputs(None,bindings,aliases);elements
                },&mut |when|when.output_bound(bindings).with_aliases(aliases));
            }
        }
    }
    pub(super) fn registers(&self) -> Vec<Reg> {
        match self {
            Self::Unit => Vec::new(),
            Self::OwnedValue(value) => value.value.regs.clone(),
            Self::Value(value) => value.regs.clone(),
            Self::Dataset(data) => vec![data.root.regs[0], data.length.regs[0]],
        }
    }
    // Data.elements contains logical model slots, not registers of the current function.
    pub(super) fn join(&mut self, other: Self, stored: bool, choice: Option<&bounds::Condition>) {
        self.join_with_presence(other, stored, choice, None);
    }

}
struct ReturnExit {
    path: bounds::Clause,
    value: Option<Returned>,
    borrowed: Result<BTreeMap<String, Returned>>,
    initialized: Vec<address::InitializationEffect>,
}
pub(super) struct ReturnContext {
    instance: crate::generics::Instance,
    expected: Arc<Expected>,
    scoped: bool,
    slots: Vec<usize>,
    exits: Vec<ReturnExit>,
    inputs: BTreeMap<String, Value>,
}
// 外层函数体的原状态按值交接，不复制当前返回模型或资源树。
struct BodyState {
    entry: usize,
    scoped: bool,
    initial: BTreeSet<Position>,
    caller: Option<ReturnContext>,
    flow: Flow,
    uniform: bool,
    code_start: usize,
}
impl ReturnContext {
    pub(super) fn exit_count(&self) -> usize {
        self.exits.len()
    }
    pub(super) fn select_exits(&mut self, first: usize, yes_end: usize, truth: bool) {
        if truth {
            self.exits.truncate(yes_end);
        } else {
            self.exits.drain(first..yes_end);
        }
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.return_scope::early_returns"]
    fn source_early_returns(
        nodes: *const std::ffi::c_void,
        source: *const std::ffi::c_void,
        stored: *const std::ffi::c_void,
        body: crate::syntax::wire::Slice,
    ) -> bool;
}
fn early_returns(body: &Block) -> bool {
    let storage = body.source_view().storage();
    // The original sealed source owner remains borrowed through this query.
    // Only the node and text regions actually consumed by B cross this ABI.
    unsafe {
        source_early_returns(storage.nodes.cast_const(), storage.source.cast_const(),
            storage.text.cast_const(), body.source_slice())
    }
}
impl Builder<'_> {
    fn inherit_recursive_element_ranges(&mut self, name: &str, code_start: usize, exits: &mut [ReturnExit]) {
        recursive::inherit(name, &self.view_model.calls, &mut self.code[code_start..], exits);
    }

    fn bind_return_exit(
        &mut self,
        value: &mut Option<Returned>,
        borrowed: &mut BTreeMap<String, Returned>,
        initialized: &mut [address::InitializationEffect],
        path: &mut bounds::Clause,
        registers: &[Reg],
        written: &BTreeMap<String, Vec<Reg>>,
        entry: usize,
    ) {
        let outputs = value.iter().map(|value| (value, registers)).chain(
            borrowed.iter().map(|(name, owner)| (owner, written[name].as_slice())),
        );
        let proofs = outputs.map(|(owner, registers)| {
            (bounds::Proof::capture(&self.returned_storage(owner)), registers)
        }).collect::<Vec<_>>();
        let mut bindings = bounds::OutputBindings::new();
        for (proof, registers) in &proofs {
            proof.output_bindings(registers, entry, &mut bindings);
        }
        let mut aliases = bounds::Clause::default();
        for (proof, registers) in proofs {
            aliases = bounds::Clause::both(
                &aliases,
                &proof.output_aliases(registers, &self.registers, &bindings),
            );
        }
        for effect in initialized {*effect=effect.output_bound(&bindings);}
        *path = bounds::Clause::both(&path.output_bound(&bindings), &aliases);
        if let Some(value) = value {
            value.bind_outputs(Some(registers), &bindings, &aliases);
        }
        for (name, owner) in borrowed.iter_mut() {
            owner.bind_outputs(Some(&written[name]),&bindings,&aliases);
        }
    }
    pub(super) fn ordinary_return(
        &self,
        result: Option<Returned>,
        span: Span,
    ) -> Result<Option<Value>> {
        match result {
            Some(Returned::Unit) => Ok(None),
            Some(Returned::OwnedValue(_)) => self.fail(
                span,
                "B224",
                "拥有结果不能按普通值复制；使用绑定、match、try或拥有参数交接",
            ),
            Some(Returned::Value(value)) => Ok(Some(*value)),
            None => Ok(None),
            Some(Returned::Dataset(_)) => self.fail(
                span,
                "B224",
                "拥有的 dataset 需要资源声明或资源返回位置，不能当作普通值",
            ),
        }
    }
    pub(super) fn result_return_type(&self) -> Option<&Ty> {
        match self.return_context.as_ref()?.expected.as_ref() {
            Expected::Value(ty) => Some(ty),
            _ => None,
        }
    }
    pub(super) fn defer_dataset_output(&mut self,data:&application::Data,positions:Vec<Position>,span:Span)->Result<bool> {
        if data.access!=Access::Write||self.scope.execution()!=crate::execution::Requirement::Invocation {
            return self.defer_position_initialization(positions,"B229",span);
        }
        let read = address::InitializationRead {
            positions: positions.into_iter().collect(),
            initialized: self.initialized.clone(),
            dataset_output: address::DatasetOutput::new(data),
        };
        self.check_view_requirement(view::Requirement::Initialized{borrow:read.borrow(),read,code:"B229",span})?;
        Ok(true)
    }
    fn publish_task_return_sources(&mut self,value:&mut Value,address:[Reg;2]) {
        let index=self.constant(Scalar::U32,0).regs[0];
        for (slot,component) in value.ty.task_components().into_iter().enumerate() {
            let mut borrow=value.proof.view_borrow(component);
            let sources=std::mem::take(&mut borrow.sources);
            let active=sources.entries().filter(|(_,when)|self.path.overlaps(when)).collect::<Vec<_>>();
            let closed=!borrow.inputs.entries().any(|(_,when)|self.path.overlaps(&when))
                && !borrow.calls.entries().any(|(_,when)|self.path.overlaps(&when))
                && !borrow.reads.entries().any(|(_,when)|self.path.overlaps(&when));
            let from_inputs=active.iter().all(|(source,_)|
                matches!(source,view::Source::Parameter(ValueIdentity::Register(_),_)));
            let exit=self.reg(Scalar::U32);
            self.emit(InstKind::Constant(exit,Scalar::U32,exit as Bits));
            let mut result=exit;
            for (source,when) in sources.entries() {
                let mut origin=exit;
                if closed && active.len()>1 && self.path.overlaps(&when) {
                    let condition=if from_inputs
                        && let view::Source::Parameter(ValueIdentity::Register(input),_)=source {
                        let selected=self.reg(Scalar::Bool);
                        self.emit(InstKind::Binary {
                            dst:selected,op:BinaryOp::Compare(Comparison::Equal),a:value.regs[component],b:*input,
                        });
                        Some(selected)
                    }else{
                        self.task_source_presence(source, &active)
                            .map(|presence| self.presence_register(&presence, None))
                    };
                    if let Some(condition)=condition {
                        origin=self.reg(Scalar::U32);
                        self.emit(InstKind::Constant(origin,Scalar::U32,origin as Bits));
                        let previous=result;
                        result=self.reg(Scalar::U32);
                        self.emit(InstKind::Select{dst:result,condition,a:origin,b:previous});
                    }
                }
                let when=bounds::Clause::both(&when,&bounds::Clause::task_return_source(component,origin as u32));
                borrow.sources.insert_when(source.clone(),when);
            }
            self.emit(InstKind::StoreIndexed{
                source:IndexedSource::View{address,index,stride:ElementStride::Constant(0)},
                offset:index,word:slot as u32*Scalar::U32.size(),value:result,
            });
            value.proof.set_view_borrow(component,borrow);
        }
    }
}



// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Returned);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        unit: ReadCallback,
        value: ReadCallback,
        dataset: ReadCallback,
        owned_value: ReadCallback,
    }
    unsafe extern "C" fn build_unit(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Returned>() = Returned::Unit;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_value(context: Slot, destination: Slot) -> ReadStatus {
        let mut value = <Box<Value> as Native>::empty();
        #[repr(C)]
        struct Fields {
            value: Slot,
        }
        #[repr(C)]
        struct Host {
            value: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_returned_value"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    value: contracts::slot(&mut value),
                },
                Host {
                    value: contracts::read_value::<Box<Value>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Returned>() = Returned::Value(value);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_dataset(context: Slot, destination: Slot) -> ReadStatus {
        let mut data = <Box<application::DataPayload> as Native>::empty();
        #[repr(C)]
        struct Fields {
            data: Slot,
        }
        #[repr(C)]
        struct Host {
            data: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_returned_dataset"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    data: contracts::slot(&mut data),
                },
                Host {
                    data: contracts::read_value::<Box<application::DataPayload>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Returned>() = Returned::Dataset(data);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_owned_value(context: Slot, destination: Slot) -> ReadStatus {
        let mut value = <Box<owned_values::OwnedValuePayload> as Native>::empty();
        #[repr(C)]
        struct Fields {
            value: Slot,
        }
        #[repr(C)]
        struct Host {
            value: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_returned_owned_value"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    value: contracts::slot(&mut value),
                },
                Host {
                    value: contracts::read_value::<Box<owned_values::OwnedValuePayload>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Returned>() = Returned::OwnedValue(value);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_returned"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Returned {
        fn empty() -> Self {
            Self::Unit
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Unit => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_returned_unit"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Value(value) => {
                    #[repr(C)]
                    struct Fields {
                        value: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        value: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_returned_value"]
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
                                value: contracts::address(value).cast_mut(),
                            },
                            Host {
                                value: contracts::write_value::<Box<Value>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Dataset(data) => {
                    #[repr(C)]
                    struct Fields {
                        data: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        data: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_returned_dataset"]
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
                                data: contracts::address(data).cast_mut(),
                            },
                            Host {
                                data: contracts::write_value::<Box<application::DataPayload>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::OwnedValue(value) => {
                    #[repr(C)]
                    struct Fields {
                        value: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        value: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_returned_owned_value"]
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
                                value: contracts::address(value).cast_mut(),
                            },
                            Host {
                                value: contracts::write_value::<Box<owned_values::OwnedValuePayload>>,
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
                        unit: build_unit,
                        value: build_value,
                        dataset: build_dataset,
                        owned_value: build_owned_value,
                    },
                )
            }
        }
    }
};
