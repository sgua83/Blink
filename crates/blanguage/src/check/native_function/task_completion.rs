//! B owns task completion cleanup order, gates and owner traversal.
use super::*;
use crate::check::application::TaskOutputSelection;
#[derive(Clone,Copy)]
#[repr(C)] struct Cursor{phase:u32,index:u64,owner:u64}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct Facts{result_owns:u32,positions:u64,datasets:u64,ready:u32,owners:u64,writes:u32,nonempty:u32}
#[repr(u32)] enum Need{Status=0,Zero=1,BeginCompleted=2,DropResult=3,Position=4,Address=5,PositionOwners=6,OwnerGuard=7,DropOwner=8,EndOwner=9,DropPosition=10,EndCompleted=11,CompletedIf=12,Dataset=13,BeginDataset=14,Descriptor=15,Guards=16,DatasetOwners=17,Guard=18,DropDataset=19,EndDataset=20,Error=21,Raise=22,Complete=23,Next=24}
#[repr(C)] struct Step{cursor:Cursor,need:Need}
unsafe extern "C"{
 #[link_name="bootstrap.task_completion::start"]fn start()->Cursor;
 #[link_name="bootstrap.task_completion::step"]fn step(state:Cursor,facts:Facts)->Step;
}
impl Builder<'_>{
 pub(in crate::check) async fn task_completion_cleanup(
  &mut self, positions: &[(u32, &Ty, Option<usize>)], datasets: &[(u32, &Ty, &Ty, Access, Option<usize>)],
  selection: &[TaskOutputSelection<'_>], result: &Ty, span: Span,
 )->Result<String>{
  let name=format!("{}::@task_completion_cleanup{}",self.scope.module(),self.functions.units.len());
  let pointer=Ty::vector(Scalar::U32,2);
  let inputs=[("output",pointer.clone()),("input",pointer),("status",Ty::scalar(Scalar::U64)),("completed",Ty::scalar(Scalar::Bool))];
  unsafe{self.world.call(self.task_callback(name,&inputs,true,span,async |b,registers|{
   let [out_low,out_high,in_low,in_high,status,completed]=*registers else{unreachable!()};
   let(output,input)=([out_low,out_high],[in_low,in_high]);
   let mut cursor=unsafe{start()};
   let mut facts=Facts{result_owns:u32::from(result.owns_resources()),positions:positions.len()as u64,datasets:datasets.len()as u64,..Facts::default()};
   let mut slot=None;let mut zero=None;let mut error=None;
   let mut outer=Vec::new();let mut guarded=Vec::new();let mut yes=Vec::new();
   let mut address=[0;2];let mut ready = None;let mut condition=None;let mut descriptor=Vec::new();let mut guards=Vec::new();
   let load=|b:&mut Builder<'_>,zero,address,scalar,word|{
    let dst=b.reg(scalar);
    b.emit(InstKind::LoadIndexed{dst,source:IndexedSource::Pointer{address,index:zero,stride:1,aliases:Vec::new()},offset:zero,word});dst
   };
   loop{let next=unsafe{step(cursor,facts)};cursor=next.cursor;match next.need{
    Need::Status=>{
     let at=b.local_slots(&Ty::scalar(Scalar::U64)).0[0];slot=Some(at);
     b.emit(InstKind::StoreLocal(at,status));b.pending_error=Some(PendingError::Exit(at));
    }
    Need::Zero=>{zero=Some(b.constant(Scalar::U32,0).regs[0]);}
    Need::BeginCompleted=>{outer=std::mem::take(&mut b.code);}
    Need::DropResult=>{unsafe{b.world.call(b.drop_task_completion_value(result,output,0,span)).await}?;}
    Need::Position => {
     ready = positions[cursor.index as usize].2.map(|index| &selection[index]);
     facts.ready = u32::from(ready.is_some());
    }
    Need::Address=>{
     let packet=positions[cursor.index as usize].0;
     address=std::array::from_fn(|i|load(b,unsafe{zero.unwrap_unchecked()},input,Scalar::U32,packet+i as u32*4));
    }
    Need::PositionOwners | Need::DatasetOwners => {
     facts.owners = unsafe { ready.unwrap_unchecked() }.owners.len() as u64;
    }
    Need::OwnerGuard=>{
     let base = unsafe { ready.unwrap_unchecked() }.base;
     condition=Some(load(b,unsafe{zero.unwrap_unchecked()},output,Scalar::Bool,base+cursor.owner as u32*Scalar::Bool.size()));
     guarded=std::mem::take(&mut b.code);
    }
    Need::DropOwner=>{
     let (offset, owner) = unsafe { ready.unwrap_unchecked() }.owners[cursor.owner as usize];
     unsafe{b.world.call(b.drop_task_completion_value(owner,address,offset,span)).await}?;
    }
    Need::EndOwner=>{
     let yes=std::mem::replace(&mut b.code,std::mem::take(&mut guarded));
     b.emit(InstKind::If{condition:unsafe{condition.take().unwrap_unchecked()},yes,no:Vec::new()});
    }
    Need::DropPosition=>{unsafe{b.world.call(b.drop_task_completion_value(positions[cursor.index as usize].1,address,0,span)).await}?;}
    Need::EndCompleted=>{yes=std::mem::replace(&mut b.code,std::mem::take(&mut outer));facts.nonempty=u32::from(!yes.is_empty());}
    Need::CompletedIf=>{b.emit(InstKind::If{condition:completed,yes:std::mem::take(&mut yes),no:Vec::new()});}
    Need::Dataset=>{
     let item = datasets[cursor.index as usize];
     ready = item.4.map(|index| &selection[index]);
     facts.ready = u32::from(ready.is_some());
     facts.writes = u32::from(item.3 == Access::Write);
    }
    Need::BeginDataset=>{outer=std::mem::take(&mut b.code);}
    Need::Descriptor=>{
     let(packet,layout,..)=datasets[cursor.index as usize];
     descriptor=layout.components.iter().map(|&(scalar,offset)|load(b,unsafe{zero.unwrap_unchecked()},input,scalar,packet+offset)).collect();
    }
    Need::Guards=>{guards=vec![owned_presence::Presence::All;datasets[cursor.index as usize].2.components.len()];}
    Need::Guard=>{
     let element = datasets[cursor.index as usize].2;
     let item = unsafe { ready.unwrap_unchecked() };
     let offset = item.owners[cursor.owner as usize].0;
     let reg=load(b,unsafe{zero.unwrap_unchecked()},output,Scalar::Bool,item.base+cursor.owner as u32*Scalar::Bool.size());
     let component=element.components.iter().position(|&(_,at)|at==offset).unwrap();
     guards[component]=owned_presence::Presence::input(&Value{
      ty:Ty::scalar(Scalar::Bool),regs:vec![reg],proof:bounds::Proof::default().with_identity(ValueIdentity::Register(reg)),
      uniform:false,literal:false,constant:None,
     });
    }
    Need::DropDataset=>{unsafe{b.world.call(b.drop_task_completion_dataset(datasets[cursor.index as usize].2,&descriptor,&guards,span)).await}?;}
    Need::EndDataset=>{
     let yes=std::mem::replace(&mut b.code,std::mem::take(&mut outer));
     b.emit(InstKind::If{condition:completed,yes,no:Vec::new()});
    }
    Need::Error=>{let reg=b.reg(Scalar::U64);error=Some(reg);b.emit(InstKind::LoadLocal(reg,unsafe{slot.unwrap_unchecked()}));}
    Need::Raise=>{b.emit(InstKind::Raise(unsafe{error.take().unwrap_unchecked()}));}
    Need::Complete=>return Ok(()),
    Need::Next=>{},
   }}
  })).await}
 }
}
