use super::*;

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Form { Unknown, TileReduce, TileScan, Reduce, SegmentedReduce, Scan, SegmentedScan }

#[repr(C)]
pub(super) struct Builtin {
    pub(super) form: Form,
    pub(super) operation: CollectiveOperator,
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::source_collective"]
    fn source_collective(bytes: *const u8, length: usize) -> Builtin;
    #[link_name = "bootstrap.operator_types::collective_accepts"]
    fn collective_accepts(operation: CollectiveOperator, numeric: u32, integer: u32) -> u32;
}

pub(super) fn from_source(name: &str) -> Builtin {
    unsafe { source_collective(name.as_ptr(), name.len()) }
}

impl Builtin {
    pub(super) fn is_tile(&self) -> bool {
        matches!(self.form, Form::TileReduce | Form::TileScan)
    }

    pub(super) fn is_stage(&self) -> bool {
        matches!(self.form, Form::Reduce | Form::SegmentedReduce | Form::Scan | Form::SegmentedScan)
    }

    pub(super) fn tile(self) -> Option<TileOp> {
        use CollectiveOperator::*;
        use Form::*;
        Some(match (self.form, self.operation) {
            (TileReduce, Sum) => TileOp::ReduceSum,
            (TileReduce, Min) => TileOp::ReduceMin,
            (TileReduce, Max) => TileOp::ReduceMax,
            (TileReduce, BitAnd) => TileOp::ReduceBitAnd,
            (TileReduce, BitOr) => TileOp::ReduceBitOr,
            (TileReduce, BitXor) => TileOp::ReduceBitXor,
            (TileScan, Sum) => TileOp::ScanExclusiveSum,
            (TileScan, Min) => TileOp::ScanExclusiveMin,
            (TileScan, Max) => TileOp::ScanExclusiveMax,
            (TileScan, BitAnd) => TileOp::ScanExclusiveBitAnd,
            (TileScan, BitOr) => TileOp::ScanExclusiveBitOr,
            (TileScan, BitXor) => TileOp::ScanExclusiveBitXor,
            _ => return None,
        })
    }
}

pub(super) fn accepts(operation: CollectiveOperator, ty: &Ty) -> bool {
    let numeric = ty.is_numeric();
    let integer = numeric && ty.components[0].0.is_integer();
    unsafe { collective_accepts(operation, u32::from(numeric), u32::from(integer)) != 0 }
}

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum NumericKind { Reduce, Scan, SegmentedScan, SegmentedReduce }
#[allow(dead_code)]
#[repr(u32)]
enum NumericNeed { Source, Target, Head, Count, Types, Access, TargetShape, CountShape, Capacity, Fields, CountWrite, GrantInput, GrantOutput, Reads, Publish, Error }
#[allow(dead_code)]
#[repr(u32)]
enum NumericError { ReduceArity, ScanArity, SegmentedArity, ReduceType, ScanType, SegmentedType, HeadAccess, ReduceAccess, ScanAccess, SegmentedAccess, ReduceShape, CountShape, ScanCapacity, SegmentedCapacity }
#[allow(dead_code)]
#[repr(u32)]
enum NumericOperation { Reduce, Scan, SegmentedReduce }
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct NumericFacts {
    arguments:u64,source:u64,target:u64,count:u64,prefix_count:u64,heads:u64,head_count:u64,
    has_prefix:u32,has_head_prefix:u32,prefix_word:u32,head_count_word:u32,
    source_access:u32,target_access:u32,head_access:u32,count_access:u32,
    numeric:u32,integer:u32,same:u32,source_overlap:u32,head_overlap:u32,
    source_known:u32,source_size:u32,target_known:u32,target_size:u32,count_known:u32,count_size:u32,
}
#[repr(C)]
struct NumericStep { cursor:SourceValueCursor,need:NumericNeed,error:NumericError,operation:NumericOperation,argument:u64 }
#[repr(C)]
struct NumericComponent { offset:u32,size:u32 }
#[repr(C)]
struct NumericReads { first:u64,second:u64,count:u64 }
unsafe extern "C" {
    #[link_name="bootstrap.function_body::numeric_step"]
    fn numeric_step(kind:NumericKind,operation:CollectiveOperator,cursor:SourceValueCursor,facts:NumericFacts)->NumericStep;
    #[link_name="bootstrap.function_body::numeric_write_fields"]
    fn numeric_write_fields(ty:*const std::ffi::c_void,context:*mut std::ffi::c_void,base:u32,count:u64,
        component:unsafe extern "C" fn(*const std::ffi::c_void,u64)->NumericComponent,
        write:unsafe extern "C" fn(*mut std::ffi::c_void,u32,u32));
    #[link_name="bootstrap.function_body::numeric_write_count"]
    fn numeric_write_count(context:*mut std::ffi::c_void,word:u32,write:unsafe extern "C" fn(*mut std::ffi::c_void,u32,u32));
    #[link_name="bootstrap.function_body::numeric_reads"]
    fn numeric_reads(source:u64,present:u32,count:u64)->NumericReads;
}

struct NumericWrite<'a> {
    program:&'a Program,facts:&'a mut prefix::Facts,binding:usize,
    regions:&'a mut BTreeMap<usize,BTreeSet<(u32,u32)>>,
}
unsafe extern "C" fn numeric_component(ty:*const std::ffi::c_void,index:u64)->NumericComponent {
    let ty=unsafe{&*ty.cast::<Ty>()};let (scalar,offset)=ty.components[index as usize];
    NumericComponent{offset,size:scalar.size()}
}
unsafe extern "C" fn numeric_written(context:*mut std::ffi::c_void,start:u32,end:u32) {
    let context=unsafe{&mut *context.cast::<NumericWrite<'_>>()};
    context.regions.entry(context.binding).or_default().insert((start,end));
    context.facts.written(context.program,context.binding,start,end,None);
}

pub(super) fn compile(
    program:&mut Program,env:&BTreeMap<String,Root>,operator:CollectiveOperator,kind:NumericKind,
    args:&Expressions,d:&Definition,span:Span,facts:&mut prefix::Facts,
    operation_source:Option<Arc<SourceSite>>,
)->Result<()> {
    let mut cursor=SourceValueCursor{phase:0};
    let mut control=NumericFacts{arguments:args.len() as u64,..Default::default()};
    let mut source=None;let mut target=None;let mut head=None;let mut count=None;
    let mut write_regions=BTreeMap::new();let mut prefix_guarantees=Vec::new();let mut reads=Vec::new();
    loop {
        let step=unsafe{numeric_step(kind,operator,cursor,control)};cursor=step.cursor;
        match step.need {
            NumericNeed::Source=>{
                let value=prefix::typed_field(program,env,&args.at(step.argument as usize),d,facts)?;
                control.source=value.0 as u64;control.source_access=value.2 as u32;
                if let Some(prefix)=value.3 {control.has_prefix=1;control.prefix_count=prefix.binding as u64;control.prefix_word=prefix.word;}
                source=Some(value);
            },
            NumericNeed::Target=>{
                let value=collective_value_field(program,env,&args.at(step.argument as usize),d)?;
                control.target=value.0 as u64;control.target_access=value.2 as u32;target=Some(value);
            },
            NumericNeed::Head=>{
                let value=prefix::field(program,env,&args.at(step.argument as usize),d,facts,Scalar::Bool)?;
                control.heads=value.0 as u64;control.head_access=value.2 as u32;
                if let Some(prefix)=value.3 {control.has_head_prefix=1;control.head_count=prefix.binding as u64;control.head_count_word=prefix.word;}
                head=Some(value);
            },
            NumericNeed::Count=>{
                let value=collective_field(program,env,&args.at(step.argument as usize),d,Scalar::U32)?;
                control.count=value.0 as u64;control.count_access=value.2 as u32;count=Some(value);
            },
            NumericNeed::Types=>{
                let ty=&source.as_ref().unwrap().4;control.numeric=u32::from(ty.is_numeric());
                control.integer=u32::from(ty.is_numeric()&&ty.components[0].0.is_integer());
                control.same=u32::from(*ty==target.as_ref().unwrap().3);
            },
            NumericNeed::Access=>{
                let (source,word,_,_,ty)=source.as_ref().unwrap();let (target,target_word,_,target_ty)=target.as_ref().unwrap();
                control.source_overlap=u32::from(prefix::fields_overlap(*source,*word,ty,*target,*target_word,target_ty));
                if let Some((heads,word,_,_))=head {control.head_overlap=u32::from(prefix::fields_overlap(heads,word,&Ty::scalar(Scalar::Bool),*target,*target_word,target_ty));}
            },
            NumericNeed::TargetShape=>{
                let size=program.buffers[target.as_ref().unwrap().0].count;
                control.target_known=u32::from(size.is_some());control.target_size=size.unwrap_or(0);
            },
            NumericNeed::CountShape=>{
                let size=program.buffers[count.unwrap().0].count;
                control.count_known=u32::from(size.is_some());control.count_size=size.unwrap_or(0);
            },
            NumericNeed::Capacity=>{
                let source_size=program.buffers[source.as_ref().unwrap().0].count;
                let target_size=program.buffers[target.as_ref().unwrap().0].count;
                control.source_known=u32::from(source_size.is_some());control.source_size=source_size.unwrap_or(0);
                control.target_known=u32::from(target_size.is_some());control.target_size=target_size.unwrap_or(0);
            },
            NumericNeed::Fields=>{
                let ty=&source.as_ref().unwrap().4;let (binding,base,_,_)=target.as_ref().unwrap();
                let mut context=NumericWrite{program,facts,binding:*binding,regions:&mut write_regions};
                unsafe{numeric_write_fields((ty as *const Ty).cast(),(&mut context as *mut NumericWrite<'_>).cast(),*base,ty.components.len() as u64,numeric_component,numeric_written)};
            },
            NumericNeed::CountWrite=>{
                let (binding,word,_)=count.unwrap();let mut context=NumericWrite{program,facts,binding,regions:&mut write_regions};
                unsafe{numeric_write_count((&mut context as *mut NumericWrite<'_>).cast(),word,numeric_written)};
            },
            NumericNeed::GrantInput=>{
                let relation=PrefixRelation{domain_region:None,domain:target.as_ref().unwrap().0,count:source.as_ref().unwrap().3.unwrap()};
                facts.grant(relation);prefix_guarantees.push(relation);
            },
            NumericNeed::GrantOutput=>{
                let (binding,word,_)=count.unwrap();
                let relation=PrefixRelation{domain_region:None,domain:target.as_ref().unwrap().0,count:PrefixCount{binding,word}};
                facts.grant(relation);prefix_guarantees.push(relation);
            },
            NumericNeed::Reads=>{
                let value=unsafe{numeric_reads(control.source,control.has_prefix,control.prefix_count)};
                reads.extend_from_slice(&[value.first as usize,value.second as usize][..value.count as usize]);
            },
            NumericNeed::Publish=>{
                let (source,source_word,_,prefix,ty)=source.unwrap();let (target,target_word,_,_)=target.unwrap();
                let fields=Reduction{source,source_word,target,target_word};let mut writes=vec![target];
                let operation=match step.operation {
                    NumericOperation::Reduce=>Operation::Reduce(ReductionSpec{fields,ty,operator}),
                    NumericOperation::Scan=>Operation::ScanExclusive(Scan{values:fields,ty,operator,head_word:head.map(|(_,word,_,_)|word)}),
                    NumericOperation::SegmentedReduce=>{
                        let (binding,word,_)=count.unwrap();writes.push(binding);
                        Operation::SegmentedReduce(SegmentedReduction{values:ReductionSpec{fields,ty,operator},head_word:head.unwrap().1,count:PrefixCount{binding,word}})
                    },
                };
                program.stages.push(Stage{operation_source,generated_origin:None,domain:source,prefix,selection:None,operation,boundary_before:true,
                    registers:Vec::new(),locals:Default::default(),code:Vec::new(),reads:reads.clone(),writes,write_regions,prefix_guarantees,
                    element_guarantees:Vec::new(),cross_access:reads,tiles:Vec::new()});
                return Ok(());
            },
            NumericNeed::Error=>{
                let (code,message)=match step.error {
                    NumericError::ReduceArity=>("B169","归约需要源数值字段和输出字段"),
                    NumericError::ScanArity=>("B169","扫描需要源字段和输出字段；分段扫描还需要源记录的 bool 段起点字段"),
                    NumericError::SegmentedArity=>("B169","分段归约需要源数值、同源 bool 段头、目标数值和 u32 计数字段"),
                    NumericError::ReduceType=>("B175","归约需要相同类型的数值标量／向量字段；按位归约只接受整数"),
                    NumericError::ScanType=>("B175","扫描需要同类型数值标量／向量字段；按位算子只接受整数"),
                    NumericError::SegmentedType=>("B175","分段归约需要同类型数值标量／向量；按位算子只接受整数"),
                    NumericError::HeadAccess=>("B170","分段起点必须来自同一可读输入记录，和源值共享元素域"),
                    NumericError::ReduceAccess=>("B170","归约需要可读输入和可写输出，实际字段不能重叠或改写输入域计数"),
                    NumericError::ScanAccess=>("B170","扫描需要可读输入和可写输出，目标不能覆盖源值、段头或输入域计数"),
                    NumericError::SegmentedAccess=>("B170","分段归约需要同域可读数值与段头、非重叠可写目标及独立输出计数"),
                    NumericError::ReduceShape=>("B171","归约输出形状必须为 1"),
                    NumericError::CountShape=>("B171","分段归约计数必须为形状 1 的 u32 计数字段"),
                    NumericError::ScanCapacity=>("B186","扫描输出形状必须满足输入域容量"),
                    NumericError::SegmentedCapacity=>("B188","分段归约目标容量必须容纳最坏情况下的全部输入段"),
                };
                let span=match step.error{NumericError::CountShape=>args.at(3).span(),_=>span};
                return d.source.fail(span,code,message);
            },
        }
    }
}
