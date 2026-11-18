module bootstrap.task_completion;
extern "C" Cursor start() on cpu {return Cursor(0,0u64,0u64);}
extern "C" Step step(Cursor state,Facts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){cursor.phase=1;return Step(cursor,Need::Status);}
 if(cursor.phase==1){cursor.phase=2;return Step(cursor,Need::Zero);}
 if(cursor.phase==2){
  if(facts.result_owns!=0||facts.positions!=0u64){cursor.phase=3;return Step(cursor,Need::BeginCompleted);}
  cursor.phase=20;return Step(cursor,Need::Next);
 }
 if(cursor.phase==3){cursor.phase=5;if(facts.result_owns!=0){return Step(cursor,Need::DropResult);}return Step(cursor,Need::Next);}
 if(cursor.phase==5){
  if(cursor.index<facts.positions){cursor.phase=6;return Step(cursor,Need::Position);}
  cursor.phase=15;return Step(cursor,Need::EndCompleted);
 }
 if(cursor.phase==6){cursor.phase=7;return Step(cursor,Need::Address);}
 if(cursor.phase==7){
  if(facts.ready!=0){cursor.phase=8;return Step(cursor,Need::PositionOwners);}
  cursor.phase=14;return Step(cursor,Need::DropPosition);
 }
 if(cursor.phase==8){cursor.owner=facts.owners;cursor.phase=9;return Step(cursor,Need::Next);}
 if(cursor.phase==9){
  if(cursor.owner!=0u64){cursor.owner-=1u64;cursor.phase=10;return Step(cursor,Need::OwnerGuard);}
  cursor.index+=1u64;cursor.phase=5;return Step(cursor,Need::Next);
 }
 if(cursor.phase==10){cursor.phase=11;return Step(cursor,Need::DropOwner);}
 if(cursor.phase==11){cursor.phase=9;return Step(cursor,Need::EndOwner);}
 if(cursor.phase==14){cursor.index+=1u64;cursor.phase=5;return Step(cursor,Need::Next);}
 if(cursor.phase==15){cursor.phase=16;if(facts.nonempty!=0){return Step(cursor,Need::CompletedIf);}return Step(cursor,Need::Next);}
 if(cursor.phase==16){cursor.index=0u64;cursor.phase=20;return Step(cursor,Need::Next);}
 if(cursor.phase==20){
  if(cursor.index<facts.datasets){cursor.phase=21;return Step(cursor,Need::Dataset);}
  cursor.phase=40;return Step(cursor,Need::Next);
 }
 if(cursor.phase==21){cursor.phase=22;if(facts.writes!=0){return Step(cursor,Need::BeginDataset);}return Step(cursor,Need::Next);}
 if(cursor.phase==22){cursor.phase=23;return Step(cursor,Need::Descriptor);}
 if(cursor.phase==23){cursor.phase=24;return Step(cursor,Need::Guards);}
 if(cursor.phase==24){
  if(facts.ready!=0){cursor.phase=25;return Step(cursor,Need::DatasetOwners);}
  cursor.phase=30;return Step(cursor,Need::Next);
 }
 if(cursor.phase==25){cursor.owner=0u64;cursor.phase=26;return Step(cursor,Need::Next);}
 if(cursor.phase==26){
  if(cursor.owner<facts.owners){cursor.phase=27;return Step(cursor,Need::Guard);}
  cursor.phase=30;return Step(cursor,Need::Next);
 }
 if(cursor.phase==27){cursor.owner+=1u64;cursor.phase=26;return Step(cursor,Need::Next);}
 if(cursor.phase==30){cursor.phase=31;return Step(cursor,Need::DropDataset);}
 if(cursor.phase==31){cursor.phase=32;if(facts.writes!=0){return Step(cursor,Need::EndDataset);}return Step(cursor,Need::Next);}
 if(cursor.phase==32){cursor.index+=1u64;cursor.phase=20;return Step(cursor,Need::Next);}
 if(cursor.phase==40){cursor.phase=41;return Step(cursor,Need::Error);}
 if(cursor.phase==41){cursor.phase=42;return Step(cursor,Need::Raise);}
 return Step(cursor,Need::Complete);
}
