//! 输出与计数在同一阶段配对；接纳原语独占预留及完整记录写入。
use super::*;

pub(super) fn clear(
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    args: &Expressions,
    d: &Definition,
    span: Span,
    facts: &mut prefix::Facts,
    trigger: Option<Arc<SourceSite>>,
) -> Result<crate::module::domain::Output> {
    if args.len() != 2 {
        return d
            .source
            .fail(span, "B220", "clear_prefix(输出, 计数字段) 需要两个参数");
    }
    let (target, access) = compact::data(env, &args.at(0), d)?;
    let (count, word, count_access) = collective_field(program, env, &args.at(1), d, Scalar::U32)?;
    if !matches!(access, Access::Write | Access::InOut)
        || !matches!(count_access, Access::Write | Access::InOut)
        || target == count
        || program.buffers[count].count != Some(1)
    {
        return d.source.fail(
            span,
            "B221",
            "clear_prefix 需要独立的普通可写输出和形状 1 的 u32 计数字段",
        );
    }
    facts.written(program, count, word, word + 4, None);
    let prefix_guarantees = vec![PrefixRelation { domain_region: None,
        domain: target,
        count: PrefixCount {
            binding: count,
            word,
        },
    }];
    for &relation in &prefix_guarantees { facts.grant(relation); }
    let generated_origin = Arc::new(GeneratedOrigin { reason: GenerationReason::ClearPrefix, trigger: trigger.map(GenerationAnchor::Syntax) });
    program.stages.push(Stage {
        operation_source: None,
        generated_origin: Some(generated_origin.clone()),
        domain: count,
        prefix: None,
        selection: None,
        operation: Operation::Map,
        boundary_before: true,
        registers: vec![Scalar::U32],
        locals: Default::default(),
        code: vec![
            Inst::generated(InstKind::Constant(0, Scalar::U32, 0), generated_origin.clone()),
            Inst::generated(InstKind::Store {
                binding: count,
                index: 0,
                word,
                value: 0,
            }, generated_origin),
        ],
        reads: Vec::new(),
        writes: vec![count],
        write_regions: BTreeMap::from([(count, BTreeSet::from([(word, word + 4)]))]),
        prefix_guarantees,
        element_guarantees: Vec::new(),
        cross_access: Vec::new(),
        tiles: Vec::new(),
    });
    Ok(crate::module::domain::Output { binding: count, domain: None, words: vec![word] })
}

impl Builder<'_> {
    pub(super) async fn append(&mut self,args:&Expressions,span:Span)->Result<Option<Value>> {
        let source=args.source_view();let storage=source.storage();let arguments=args.source_slice();
        let mut cursor=LexicalCursor{phase:0,flags:0,index:0,part:0};let mut facts=AppendFacts{present:0,access:0,views:0,target_domain:0,count_domain:0,target:0,count:0,target_components:0,value_components:0,registers:0};
        let mut target=None;let mut count=None;let mut count_type=None;let mut value=None;let mut accepted=None;let mut values=Vec::new();
        loop{let step=unsafe{append_step(cursor,storage,arguments,AppendFacts{..facts})};cursor=step.cursor;match step.need{
            AppendNeed::BadArguments=>return self.fail(span,"B220","append(输出, 计数字段, 完整记录值) 需要三个参数"),
            AppendNeed::Binding=>{target=match self.env.get(source.text(step.name)){Some(Binding::Data(view))=>Some(view.binding),_=>None};facts.present=u32::from(target.is_some());if let Some(target)=target{facts.target=target as u64;}},
            AppendNeed::BadOutput=>return self.fail(args.at(0).span(),"B221","append 输出需要命名 dataset"),
            AppendNeed::Count=>{let(binding,word,ty)=self.dataset_field(&args.at(1))?;count=Some(PrefixCount{binding,word});count_type=Some(ty);facts.count=binding as u64;},
            AppendNeed::TargetAccess=>facts.access=self.view_access[&target.unwrap()]as u32,
            AppendNeed::CountAccess=>facts.access=self.view_access[&count.unwrap().binding]as u32,
            AppendNeed::Initialized=>{let count=count.unwrap();facts.present=u32::from(self.initialized.contains(&Position::Element(count.binding,count.word)));},
            AppendNeed::Shape=>facts.present=u32::from(self.program.buffers[count.unwrap().binding].count==Some(1)),
            AppendNeed::CountType=>facts.present=u32::from(count_type.as_ref().unwrap()==&Ty::scalar(Scalar::U32)),
            AppendNeed::BadContract=>return self.fail(span,"B221","append 需要普通独占元素／tile 输入域之外的可写输出及独立 inout 形状 1 资源中的 u32 计数字段；两者不能别名"),
            AppendNeed::Value=>{value=unsafe{self.world.call(self.expr(&args.at(2))).await}?;facts.present=u32::from(value.is_some());},
            AppendNeed::Empty=>return Ok(None),
            AppendNeed::ValueType=>{let value=value.as_ref().unwrap();self.require_same(&self.program.buffers[target.unwrap()].ty,&value.ty,args.at(2).span())?;facts.views=u32::from(value.ty.contains_views());facts.target_components=self.program.buffers[target.unwrap()].ty.components.len()as u64;facts.value_components=value.ty.components.len()as u64;facts.registers=value.regs.len()as u64;},
            AppendNeed::BadViews=>return self.fail(args.at(2).span(),"B185","append不能写出持久view载荷；原引用图须保持"),
            AppendNeed::Domains=>{facts.target_domain=u32::from(self.element_domains.contains(&target.unwrap()));facts.count_domain=u32::from(self.element_domains.contains(&count.unwrap().binding));},
            AppendNeed::BadDomain=>return self.fail(span,"B221","append 输出／计数不能与本阶段普通独占元素或 tile 输入域重叠"),
            AppendNeed::Pairs=>{let count=count.unwrap();let mut entries=self.appends.iter();facts.present=unsafe{append_pair_conflict(target.unwrap()as u64,count.binding as u64,count.word,&mut entries,next_position_append)};},
            AppendNeed::BadPair=>return self.fail(span,"B222","同一并行阶段的 append 输出与计数必须一对一配对"),
            AppendNeed::Pair=>{self.appends.insert(target.unwrap(),count.unwrap());},
            AppendNeed::Read=>{self.reads.insert(count.unwrap().binding);},
            AppendNeed::Writes=>{self.writes.extend([target.unwrap(),count.unwrap().binding]);},
            AppendNeed::CountRegion=>{let count=count.unwrap();self.memory_regions.entry(count.binding).or_default().1.insert((count.word,count.word+4),None);},
            AppendNeed::MemoryField=>{let(scalar,word)=self.program.buffers[target.unwrap()].ty.components[cursor.index as usize];self.memory_regions.entry(target.unwrap()).or_default().1.insert((word,word+scalar.size()),None);},
            AppendNeed::Cross=>{self.cross_access.extend([target.unwrap(),count.unwrap().binding]);},
            AppendNeed::Register=>accepted=Some(self.reg(Scalar::Bool)),
            AppendNeed::ValueField=>{let value=value.as_ref().unwrap();values.push((value.ty.components[cursor.index as usize].1,value.regs[cursor.index as usize]));},
            AppendNeed::Emit=>self.emit(InstKind::Append(Append{accepted:accepted.unwrap(),target:target.unwrap(),count:count.unwrap(),values:std::mem::take(&mut values)})),
            AppendNeed::Complete=>return Ok(Some(Value{proof:bounds::Proof::default(),literal:false,ty:Ty::scalar(Scalar::Bool),regs:vec![accepted.unwrap()],uniform:false,constant:None})),
        }}
    }
}

#[repr(C)]struct AppendFacts{present:u32,access:u32,views:u32,target_domain:u32,count_domain:u32,target:u64,count:u64,target_components:u64,value_components:u64,registers:u64}
#[repr(u32)]enum AppendNeed{BadArguments,Binding,BadOutput,Count,TargetAccess,CountAccess,Initialized,Shape,CountType,BadContract,Value,Empty,ValueType,BadViews,Domains,BadDomain,Pairs,BadPair,Pair,Read,Writes,CountRegion,MemoryField,Cross,Register,ValueField,Emit,Complete}
#[repr(C)]struct AppendStep{cursor:LexicalCursor,need:AppendNeed,name:crate::syntax::wire::Text}
unsafe extern "C"{
 #[link_name="bootstrap.function_body::append_step"]fn append_step(cursor:LexicalCursor,source:crate::syntax::wire::StorageView,arguments:crate::syntax::wire::Slice,facts:AppendFacts)->AppendStep;
 #[link_name="bootstrap.function_body::append_pair_conflict"]fn append_pair_conflict(target:u64,count:u64,word:u32,context:*mut PositionAppendIter<'_>,next:unsafe extern "C" fn(*mut PositionAppendIter<'_>)->PositionAppend)->u32;
}
